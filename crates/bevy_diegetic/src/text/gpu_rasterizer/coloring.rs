//! Alternative MSDF edge-coloring algorithm: by-distance.
//!
//! Port of msdfgen's `edgeColoringByDistance` (Chlumský,
//! `core/edge-coloring.cpp`). fdsm only implements `edge_coloring_simple`,
//! which assigns channel colors based purely on contour-corner topology.
//! That algorithm fails at sharp inner concavities ("ink traps") — narrow
//! necks where two contour edges run within ~1 texel of each other —
//! because adjacent same-color edges cause the per-channel `median(r,g,b)`
//! to flip on the wrong side, producing a wedge notch in the rasterized
//! glyph (e.g. the EB Garamond G bowl-to-link junction at 32-64px atlas
//! resolutions).
//!
//! By-distance builds a distance matrix between all splines (sampled
//! point-to-point) and runs a 3-channel graph coloring that resolves
//! spatial proximity directly: splines that are physically close get
//! constrained to different colors. Compared to the topological "ink-trap"
//! heuristic (which marks corners as minor based on adjacent spline
//! length), by-distance handles non-adjacent splines that happen to run
//! close in pixel space — e.g. the comb-pattern at the top junction of EB
//! Garamond G at 128px.
//!
//! Used by the GPU pipeline only. The CPU `fdsm` path continues to use
//! `Shape::edge_coloring_simple` so this is a GPU-only improvement. // allow-banned: upstream fdsm
//! API name

use std::collections::VecDeque;

use bevy_kana::ToI32;
use fdsm::bezier::Segment as FdsmSegment;
use fdsm::color::Color;
use fdsm::distance::DistanceField as TrueDistanceField;
use fdsm::shape::ColoredContour; // allow-banned: upstream fdsm API name
use fdsm::shape::ColoredSegment; // allow-banned: upstream fdsm API name
use fdsm::shape::Contour; // allow-banned: upstream fdsm API name
use fdsm::shape::Shape; // allow-banned: upstream fdsm API name
use nalgebra::Vector2;

fn is_corner(a_dir: Vector2<f64>, b_dir: Vector2<f64>, cross_threshold: f64) -> bool {
    let dot = a_dir.dot(&b_dir);
    let cross = a_dir.x.mul_add(b_dir.y, -(a_dir.y * b_dir.x));
    dot <= 0.0 || cross.abs() > cross_threshold
}

/// msdfgen's `symmetricalTrichotomy(position, n)` — maps position in [0, n)
/// to one of {-1, 0, 1}, biased to put 0 on the middle segments.
fn symmetrical_trichotomy(position: usize, n: usize) -> i32 {
    // Reproduce msdfgen exactly: `int(3 + 2.875*position/(n-1) - 1.4375 + 0.5) - 3`
    let p = f64::from(position.to_i32());
    let nm1 = f64::from(n.saturating_sub(1).to_i32());
    let value = 3.0 + 2.875 * p / nm1 - 1.4375 + 0.5;
    value.to_i32() - 3
}

/// Subdivisions used when sampling spline-to-spline distance. Matches
/// msdfgen's `EDGE_DISTANCE_PRECISION` (16).
const EDGE_DISTANCE_PRECISION: u32 = 16;

/// Maximum recolor steps for the constraint-propagating coloring inner
/// loop. Matches msdfgen's `MAX_RECOLOR_STEPS` (16).
const MAX_RECOLOR_STEPS: u32 = 16;

/// FIRST_POSSIBLE_COLOR[bitmask] returns the lowest-indexed color (0/1/2)
/// permitted by the 3-bit mask, where bit 0 = color 0, bit 1 = color 1,
/// bit 2 = color 2. Index 0 is unused (no color permitted); index 7 returns
/// 0 (all three permitted, pick the first).
const FIRST_POSSIBLE_COLOR: [i32; 8] = [-1, 0, 1, 0, 2, 2, 1, 0];

/// One spline (a contiguous run of edge segments between corners). The
/// by-distance algorithm assigns one color per spline based on inter-spline
/// proximity.
struct Spline {
    segments: Vec<FdsmSegment>,
}

/// Per-output-segment color assignment.
#[derive(Clone, Copy)]
enum ItemColor {
    /// Use the named color directly (e.g. teardrop middle = WHITE).
    Fixed(Color),
    /// Look up `spline_colors[index]` after graph coloring completes.
    FromSpline(usize),
}

/// Per-contour state built during the corner walk, materialized into a
/// `ColoredContour` once spline colors are known.
struct ContourBuild {
    segments: Vec<FdsmSegment>,
    colors:   Vec<ItemColor>,
}

/// Colors a glyph outline using msdfgen's `edgeColoringByDistance` algorithm. // allow-banned:
/// upstream fdsm API name
///
/// Drop-in replacement for `fdsm::shape::Shape::edge_coloring_simple` with the // allow-banned:
/// upstream fdsm API name same signature and seed semantics.
#[must_use]
pub(super) fn edge_coloring_by_distance(
    input: Shape<Contour>, // allow-banned: upstream fdsm API name
    sin_alpha: f64,
    seed: u64,
) -> Shape<ColoredContour> {
    // allow-banned: upstream fdsm API name
    let cross_threshold = sin_alpha;
    let mut seed = seed;

    // Walk each contour, identifying corners and building the global spline
    // list plus per-contour materialization plans.
    let mut builds: Vec<ContourBuild> = Vec::with_capacity(input.contours.len());
    let mut splines: Vec<Spline> = Vec::new();
    for contour in &input.contours {
        builds.push(build_contour_by_distance(
            contour,
            cross_threshold,
            &mut splines,
        ));
    }

    let spline_count = splines.len();
    if spline_count == 0 {
        return Shape {
            // allow-banned: upstream fdsm API name
            contours: builds
                .into_iter()
                .map(|b| materialize_contour(b, &[]))
                .collect(),
        };
    }

    // Distance matrix: distance[i][j] = min sampled point distance between
    // spline i and spline j. Diagonal set to -1 to match msdfgen (a sentinel
    // that prevents self-edges from appearing in the sorted edge list).
    let mut distance_matrix = vec![0.0_f64; spline_count * spline_count];
    for i in 0..spline_count {
        distance_matrix[i * spline_count + i] = -1.0;
        for j in (i + 1)..spline_count {
            let d = spline_to_spline_distance(&splines[i], &splines[j], EDGE_DISTANCE_PRECISION);
            distance_matrix[i * spline_count + j] = d;
            distance_matrix[j * spline_count + i] = d;
        }
    }

    // Sort upper-triangle pairs by distance ascending. Zero-distance pairs
    // get added to the adjacency graph unconditionally (touching splines must
    // differ in color); larger-distance pairs go through try_add_edge.
    let mut graph_edges: Vec<(usize, usize)> =
        Vec::with_capacity(spline_count.saturating_mul(spline_count.saturating_sub(1)) / 2);
    for i in 0..spline_count {
        for j in (i + 1)..spline_count {
            graph_edges.push((i, j));
        }
    }
    graph_edges.sort_by(|&(ai, aj), &(bi, bj)| {
        distance_matrix[ai * spline_count + aj].total_cmp(&distance_matrix[bi * spline_count + bj])
    });

    // Adjacency matrix populated with all zero-distance pairs first.
    let mut edge_matrix = vec![0_i32; spline_count * spline_count];
    let mut next_edge = 0_usize;
    while next_edge < graph_edges.len() {
        let (a, b) = graph_edges[next_edge];
        if distance_matrix[a * spline_count + b] != 0.0 {
            break;
        }
        edge_matrix[a * spline_count + b] = 1;
        edge_matrix[b * spline_count + a] = 1;
        next_edge += 1;
    }

    let mut coloring = vec![0_i32; spline_count];
    let mut buffer = vec![0_i32; spline_count];
    color_second_degree_graph(&mut coloring, &edge_matrix, spline_count, &mut seed);

    while next_edge < graph_edges.len() {
        let (a, b) = graph_edges[next_edge];
        try_add_edge(
            &mut coloring,
            &mut edge_matrix,
            spline_count,
            a,
            b,
            &mut buffer,
        );
        next_edge += 1;
    }

    // Map coloring indices 0/1/2 → YELLOW/CYAN/MAGENTA. Matches msdfgen's
    // `const EdgeColor colors[3] = { YELLOW, CYAN, MAGENTA };`.
    let palette = [Color::YELLOW, Color::CYAN, Color::MAGENTA];
    let spline_colors: Vec<Color> = coloring
        .iter()
        .map(|&c| {
            let idx = usize::try_from(c.clamp(0, 2)).unwrap_or(0);
            palette[idx]
        })
        .collect();

    Shape {
        // allow-banned: upstream fdsm API name
        contours: builds
            .into_iter()
            .map(|b| materialize_contour(b, &spline_colors))
            .collect(),
    }
}

fn materialize_contour(build: ContourBuild, spline_colors: &[Color]) -> ColoredContour {
    let segments = build
        .segments
        .into_iter()
        .zip(build.colors)
        .map(|(segment, item_color)| {
            let color = match item_color {
                ItemColor::Fixed(c) => c,
                ItemColor::FromSpline(idx) => spline_colors[idx],
            };
            ColoredSegment { segment, color } // allow-banned: upstream fdsm API name
        })
        .collect();
    ColoredContour { segments } // allow-banned: upstream fdsm API name
}

fn build_contour_by_distance(
    contour: &Contour, // allow-banned: upstream fdsm API name
    cross_threshold: f64,
    splines: &mut Vec<Spline>,
) -> ContourBuild {
    let n = contour.segments.len();
    if n == 0 {
        return ContourBuild {
            segments: Vec::new(),
            colors:   Vec::new(),
        };
    }

    let mut corners: Vec<usize> = Vec::new();
    let last = contour.segments[n - 1];
    let mut prev_direction = last.direction_at(1.0);
    for (i, seg) in contour.segments.iter().enumerate() {
        let cur_direction = seg.direction_at(0.0);
        if is_corner(
            prev_direction.normalize(),
            cur_direction.normalize(),
            cross_threshold,
        ) {
            corners.push(i);
        }
        prev_direction = seg.direction_at(1.0);
    }

    match corners.len() {
        0 => {
            // Smooth contour: a single spline covers every segment.
            let spline_idx = splines.len();
            splines.push(Spline {
                segments: contour.segments.clone(),
            });
            ContourBuild {
                segments: contour.segments.clone(),
                colors:   vec![ItemColor::FromSpline(spline_idx); n],
            }
        },
        1 => build_teardrop_by_distance(contour, corners[0], splines),
        _ => build_multi_corner_by_distance(contour, &corners, splines),
    }
}

fn build_teardrop_by_distance(
    contour: &Contour, // allow-banned: upstream fdsm API name
    corner: usize,
    splines: &mut Vec<Spline>,
) -> ContourBuild {
    let n = contour.segments.len();
    if n >= 3 {
        // Two splines flanking a WHITE middle. The walk starts at `corner`
        // and the spline-boundary is at i == n/2; `symmetrical_trichotomy(i,
        // n) == 0` marks the middle segments which get pinned to WHITE.
        let half = n / 2;
        let spline_a = splines.len();
        let spline_b = spline_a + 1;
        let mut seg_a: Vec<FdsmSegment> = Vec::new();
        let mut seg_b: Vec<FdsmSegment> = Vec::new();
        let segments_out = contour.segments.clone();
        let mut colors_out = vec![ItemColor::Fixed(Color::WHITE); n];
        for i in 0..n {
            let edge_index = (corner + i) % n;
            if symmetrical_trichotomy(i, n) != 0 {
                if i < half {
                    seg_a.push(contour.segments[edge_index]);
                    colors_out[edge_index] = ItemColor::FromSpline(spline_a);
                } else {
                    seg_b.push(contour.segments[edge_index]);
                    colors_out[edge_index] = ItemColor::FromSpline(spline_b);
                }
            }
        }
        splines.push(Spline { segments: seg_a });
        splines.push(Spline { segments: seg_b });
        return ContourBuild {
            segments: segments_out,
            colors:   colors_out,
        };
    }

    // n in {1, 2}: not enough segments for three colors, so split each
    // segment into thirds. `parts[7]` mirrors msdfgen's indexing where
    // `corner` (0 or 1) controls where each split's three pieces land.
    let mut parts: [Option<FdsmSegment>; 7] = [None; 7];
    let split0 = contour.segments[0].split_in_thirds();
    let base0 = 3 * corner;
    parts[base0] = Some(split0[0]);
    parts[base0 + 1] = Some(split0[1]);
    parts[base0 + 2] = Some(split0[2]);
    if n >= 2 {
        let split1 = contour.segments[1].split_in_thirds();
        let base1 = if corner == 0 { 3 } else { 0 };
        parts[base1] = Some(split1[0]);
        parts[base1 + 1] = Some(split1[1]);
        parts[base1 + 2] = Some(split1[2]);
    }

    let spline_a = splines.len();
    let spline_b = spline_a + 1;
    let mut seg_a: Vec<FdsmSegment> = Vec::new();
    let mut seg_b: Vec<FdsmSegment> = Vec::new();
    let mut segments_out: Vec<FdsmSegment> = Vec::new();
    let mut colors_out: Vec<ItemColor> = Vec::new();

    if n >= 2 {
        // parts[0..2] → spline_a, parts[2..4] → WHITE, parts[4..6] → spline_b
        for (i, slot) in parts.iter().take(6).enumerate() {
            let Some(seg) = slot else { continue };
            segments_out.push(*seg);
            if i < 2 {
                seg_a.push(*seg);
                colors_out.push(ItemColor::FromSpline(spline_a));
            } else if i < 4 {
                colors_out.push(ItemColor::Fixed(Color::WHITE));
            } else {
                seg_b.push(*seg);
                colors_out.push(ItemColor::FromSpline(spline_b));
            }
        }
    } else {
        // n == 1: parts[0] → spline_a, parts[1] → WHITE, parts[2] → spline_b
        for (i, slot) in parts.iter().take(3).enumerate() {
            let Some(seg) = slot else { continue };
            segments_out.push(*seg);
            match i {
                0 => {
                    seg_a.push(*seg);
                    colors_out.push(ItemColor::FromSpline(spline_a));
                },
                1 => colors_out.push(ItemColor::Fixed(Color::WHITE)),
                _ => {
                    seg_b.push(*seg);
                    colors_out.push(ItemColor::FromSpline(spline_b));
                },
            }
        }
    }

    splines.push(Spline { segments: seg_a });
    splines.push(Spline { segments: seg_b });

    ContourBuild {
        segments: segments_out,
        colors:   colors_out,
    }
}

fn build_multi_corner_by_distance(
    contour: &Contour, // allow-banned: upstream fdsm API name
    corners: &[usize],
    splines: &mut Vec<Spline>,
) -> ContourBuild {
    let n = contour.segments.len();
    let corner_count = corners.len();
    let start = corners[0];
    let mut current_spline: Vec<FdsmSegment> = Vec::new();
    let mut current_spline_idx = splines.len();
    let mut corner_cursor = 0_usize;
    let mut colors_out: Vec<ItemColor> = vec![ItemColor::Fixed(Color::BLACK); n];

    for i in 0..n {
        let index = (start + i) % n;
        if corner_cursor + 1 < corner_count && corners[corner_cursor + 1] == index {
            // Close current spline and open the next.
            splines.push(Spline {
                segments: std::mem::take(&mut current_spline),
            });
            current_spline_idx += 1;
            corner_cursor += 1;
        }
        colors_out[index] = ItemColor::FromSpline(current_spline_idx);
        current_spline.push(contour.segments[index]);
    }
    splines.push(Spline {
        segments: current_spline,
    });

    ContourBuild {
        segments: contour.segments.clone(),
        colors:   colors_out,
    }
}

fn spline_to_spline_distance(a: &Spline, b: &Spline, precision: u32) -> f64 {
    let mut min_distance = f64::MAX;
    for sa in &a.segments {
        for sb in &b.segments {
            if min_distance == 0.0 {
                return 0.0;
            }
            let d = edge_to_edge_distance(sa, sb, precision);
            if d < min_distance {
                min_distance = d;
            }
        }
    }
    min_distance
}

fn edge_to_edge_distance(a: &FdsmSegment, b: &FdsmSegment, precision: u32) -> f64 {
    let a0 = a.start();
    let a1 = a.end();
    let b0 = b.start();
    let b1 = b.end();
    if a0 == b0 || a0 == b1 || a1 == b0 || a1 == b1 {
        return 0.0;
    }
    let i_fac = 1.0 / f64::from(precision);
    let mut min_distance = (b0 - a0).norm();
    for i in 0..=precision {
        let t = i_fac * f64::from(i);
        let d = a
            .signed_distance_and_orthogonality::<TrueDistanceField>(b.get(t))
            .distance()
            .abs();
        if d < min_distance {
            min_distance = d;
        }
    }
    for i in 0..=precision {
        let t = i_fac * f64::from(i);
        let d = b
            .signed_distance_and_orthogonality::<TrueDistanceField>(a.get(t))
            .distance()
            .abs();
        if d < min_distance {
            min_distance = d;
        }
    }
    min_distance
}

fn seed_extract2(seed: &mut u64) -> i32 {
    let v = i32::try_from(*seed & 1).unwrap_or(0);
    *seed >>= 1;
    v
}

fn seed_extract3(seed: &mut u64) -> i32 {
    let v = i32::try_from(*seed % 3).unwrap_or(0);
    *seed /= 3;
    v
}

fn color_second_degree_graph(
    coloring: &mut [i32],
    edge_matrix: &[i32],
    vertex_count: usize,
    seed: &mut u64,
) {
    for i in 0..vertex_count {
        let mut possible_colors = 7_i32;
        for j in 0..i {
            if edge_matrix[i * vertex_count + j] != 0 {
                let c = coloring[j];
                if (0..3).contains(&c) {
                    possible_colors &= !(1 << c);
                }
            }
        }
        coloring[i] = match possible_colors {
            1 => 0,
            2 => 1,
            // 0 or 1 from a single seed bit.
            3 => seed_extract2(seed),
            4 => 2,
            // 2 or 0: msdfgen's `(int)!seedExtract2(seed)<<1`.
            5 => (1 - seed_extract2(seed)) << 1,
            // 1 or 2.
            6 => seed_extract2(seed) + 1,
            // 0, 1, or 2, offset by vertex index for variety.
            7 => (seed_extract3(seed) + i.to_i32()).rem_euclid(3),
            _ => 0,
        };
    }
}

fn vertex_possible_colors(
    coloring: &[i32],
    edge_vector_offset: usize,
    vertex_count: usize,
    edge_matrix: &[i32],
) -> i32 {
    let mut used = 0_i32;
    for i in 0..vertex_count {
        if edge_matrix[edge_vector_offset + i] != 0 {
            let c = coloring[i];
            if (0..3).contains(&c) {
                used |= 1 << c;
            }
        }
    }
    7 & !used
}

fn uncolor_same_neighbors(
    uncolored: &mut VecDeque<usize>,
    coloring: &mut [i32],
    edge_matrix: &[i32],
    vertex: usize,
    vertex_count: usize,
) {
    let cur_color = coloring[vertex];
    for i in (vertex + 1)..vertex_count {
        if edge_matrix[vertex * vertex_count + i] != 0 && coloring[i] == cur_color {
            coloring[i] = -1;
            uncolored.push_back(i);
        }
    }
    for i in 0..vertex {
        if edge_matrix[vertex * vertex_count + i] != 0 && coloring[i] == cur_color {
            coloring[i] = -1;
            uncolored.push_back(i);
        }
    }
}

fn try_add_edge(
    coloring: &mut [i32],
    edge_matrix: &mut [i32],
    vertex_count: usize,
    vertex_a: usize,
    vertex_b: usize,
    coloring_buffer: &mut [i32],
) -> bool {
    edge_matrix[vertex_a * vertex_count + vertex_b] = 1;
    edge_matrix[vertex_b * vertex_count + vertex_a] = 1;
    if coloring[vertex_a] != coloring[vertex_b] {
        return true;
    }
    let b_possible =
        vertex_possible_colors(coloring, vertex_b * vertex_count, vertex_count, edge_matrix);
    if b_possible != 0 {
        let idx = usize::try_from(b_possible).unwrap_or(0);
        coloring[vertex_b] = FIRST_POSSIBLE_COLOR[idx];
        return true;
    }

    coloring_buffer.copy_from_slice(coloring);
    let mut uncolored: VecDeque<usize> = VecDeque::new();
    {
        // Re-color into `coloring_buffer`; the original `coloring` is the
        // rollback target if propagation fails to converge.
        let a_color = coloring_buffer[vertex_a];
        let banned_mask = if (0..3).contains(&a_color) {
            7 & !(1 << a_color)
        } else {
            7
        };
        let initial_idx = usize::try_from(banned_mask).unwrap_or(0);
        coloring_buffer[vertex_b] = FIRST_POSSIBLE_COLOR[initial_idx];
        uncolor_same_neighbors(
            &mut uncolored,
            coloring_buffer,
            edge_matrix,
            vertex_b,
            vertex_count,
        );

        let mut step: u32 = 0;
        while step < MAX_RECOLOR_STEPS {
            let Some(i) = uncolored.pop_front() else {
                break;
            };
            let possible = vertex_possible_colors(
                coloring_buffer,
                i * vertex_count,
                vertex_count,
                edge_matrix,
            );
            if possible != 0 {
                let idx = usize::try_from(possible).unwrap_or(0);
                coloring_buffer[i] = FIRST_POSSIBLE_COLOR[idx];
                continue;
            }
            loop {
                coloring_buffer[i] = i32::try_from(step % 3).unwrap_or(0);
                step += 1;
                if !(edge_matrix[i * vertex_count + vertex_a] != 0
                    && coloring_buffer[i] == coloring_buffer[vertex_a])
                {
                    break;
                }
            }
            uncolor_same_neighbors(
                &mut uncolored,
                coloring_buffer,
                edge_matrix,
                i,
                vertex_count,
            );
        }
    }
    if !uncolored.is_empty() {
        edge_matrix[vertex_a * vertex_count + vertex_b] = 0;
        edge_matrix[vertex_b * vertex_count + vertex_a] = 0;
        return false;
    }
    coloring.copy_from_slice(coloring_buffer);
    true
}

#[cfg(test)]
mod tests {
    #![allow(
        clippy::panic,
        clippy::unwrap_used,
        reason = "tests use panic/unwrap for clearer failure messages"
    )]

    use ttf_parser::Face;
    use ttf_parser::GlyphId;

    use super::*;

    const EB_GARAMOND: &[u8] = include_bytes!("../../../assets/fonts/EBGaramond-Regular.ttf");
    const JETBRAINS_MONO: &[u8] = include_bytes!("../../../assets/fonts/JetBrainsMono-Regular.ttf");

    fn glyph_index(font_data: &[u8], ch: char) -> u16 {
        let face = Face::parse(font_data, 0).unwrap();
        face.glyph_index(ch).unwrap().0
    }

    fn collect_masks(font_data: &[u8], ch: char) -> Vec<u8> {
        let face = Face::parse(font_data, 0).unwrap();
        let glyph_id = GlyphId(glyph_index(font_data, ch));
        let outline = fdsm_ttf_parser::load_shape_from_face(&face, glyph_id).unwrap(); // allow-banned: upstream fdsm API name
        let colored = edge_coloring_by_distance(outline, 3.0_f64.to_radians().sin(), 0);
        let mut out = Vec::new();
        for contour in &colored.contours {
            for seg in &contour.segments {
                out.push(seg.color.value());
            }
        }
        out
    }

    #[test]
    fn by_distance_produces_non_empty_coloring_for_eb_garamond_g() {
        let masks = collect_masks(EB_GARAMOND, 'g');
        assert!(!masks.is_empty(), "expected at least one colored segment");
        for m in &masks {
            assert!(*m <= 7, "color mask out of range: {m}");
        }
    }

    #[test]
    fn by_distance_handles_simple_glyphs() {
        for (font, label, glyphs) in [
            (
                JETBRAINS_MONO,
                "JetBrains Mono",
                ['A', 'O', 'W', 'g'].as_slice(),
            ),
            (EB_GARAMOND, "EB Garamond", ['V', 'A', 'g', 'O'].as_slice()),
        ] {
            for &ch in glyphs {
                let masks = collect_masks(font, ch);
                assert!(!masks.is_empty(), "{label} '{ch}': empty coloring");
            }
        }
    }

    #[test]
    fn symmetrical_trichotomy_matches_msdfgen() {
        // Verify the formula directly for a few sample sizes.
        // For n=3: positions 0,1,2 → -1, 0, 1
        assert_eq!(symmetrical_trichotomy(0, 3), -1);
        assert_eq!(symmetrical_trichotomy(1, 3), 0);
        assert_eq!(symmetrical_trichotomy(2, 3), 1);
        // For n=5: positions span the same -1, 0, 1 range
        assert_eq!(symmetrical_trichotomy(0, 5), -1);
        assert_eq!(symmetrical_trichotomy(4, 5), 1);
    }
}
