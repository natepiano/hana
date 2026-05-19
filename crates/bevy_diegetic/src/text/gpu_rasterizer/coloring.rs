//! Alternative MSDF edge-coloring algorithm: ink-trap.
//!
//! Port of msdfgen's `edgeColoringInkTrap` (Chlumský, `core/edge-coloring.cpp`).
//! fdsm only implements `edge_coloring_simple`, which assigns channel colors
//! based purely on contour-corner topology. That algorithm fails at sharp
//! inner concavities ("ink traps") — narrow necks where two contour edges
//! run within ~1 texel of each other — because adjacent same-color edges
//! cause the per-channel `median(r,g,b)` to flip on the wrong side, producing
//! a wedge notch in the rasterized glyph (e.g. the EB Garamond G bowl-to-link
//! junction at 32-64px atlas resolutions).
//!
//! The ink-trap algorithm identifies short spline segments between corners
//! (local minima in spline-length) and marks those corners as "minor". Minor
//! corners receive colors derived from neighbors (`(color & next_color) ^
//! WHITE`) rather than from the seeded rotation, which prevents the
//! same-color collision in narrow regions.
//!
//! Used by the GPU pipeline only. The CPU `fdsm` path continues to use
//! `Shape::edge_coloring_simple` so this is a GPU-only improvement. // allow-banned: upstream fdsm
//! API name

use bevy_kana::ToI32;
use fdsm::bezier::Segment as FdsmSegment;
use fdsm::color::Color;
use fdsm::shape::ColoredContour; // allow-banned: upstream fdsm API name
use fdsm::shape::ColoredSegment; // allow-banned: upstream fdsm API name
use fdsm::shape::Contour; // allow-banned: upstream fdsm API name
use fdsm::shape::Shape; // allow-banned: upstream fdsm API name
use nalgebra::Vector2;

/// Number of subdivisions used when estimating a curved segment's arc length.
/// Matches msdfgen's `MSDFGEN_EDGE_LENGTH_PRECISION` default.
const EDGE_LENGTH_PRECISION: u32 = 4;

#[derive(Clone, Copy, Debug)]
struct InkTrapCorner {
    index:                     usize,
    prev_edge_length_estimate: f64,
    minor:                     bool,
    color:                     Color,
}

/// Colors a glyph outline using msdfgen's `edgeColoringInkTrap` algorithm. // allow-banned:
/// upstream fdsm API name
///
/// Drop-in replacement for `fdsm::shape::Shape::edge_coloring_simple` with the // allow-banned:
/// upstream fdsm API name same signature and seed semantics.
#[must_use]
pub(super) fn edge_coloring_ink_trap(
    input: Shape<Contour>, // allow-banned: upstream fdsm API name
    sin_alpha: f64,
    seed: u64,
) -> Shape<ColoredContour> {
    // allow-banned: upstream fdsm API name
    let cross_threshold = sin_alpha;
    // msdfgen `initColor(seed)` — picks one of {CYAN, MAGENTA, YELLOW} based
    // on `seed % 3`. fdsm's `Color::WHITE.switch(seed, BLACK)` triggers the
    // same BLACK/WHITE initial branch that returns one of those three.
    let (mut color, mut seed) = Color::WHITE.switch(seed, Color::BLACK);

    let mut out_contours = Vec::with_capacity(input.contours.len());
    for contour in input.contours {
        let (colored, next_color, next_seed) =
            color_contour_ink_trap(contour, cross_threshold, color, seed);
        color = next_color;
        seed = next_seed;
        out_contours.push(colored);
    }
    Shape {
        contours: out_contours,
    } // allow-banned: upstream fdsm API name
}

fn color_contour_ink_trap(
    contour: Contour, // allow-banned: upstream fdsm API name
    cross_threshold: f64,
    color_in: Color,
    seed_in: u64,
) -> (ColoredContour, Color, u64) {
    let mut color = color_in;
    let mut seed = seed_in;

    let Some(last_segment) = contour.segments.last() else {
        return (
            ColoredContour {
                segments: Vec::new(),
            },
            color,
            seed,
        ); // allow-banned: upstream fdsm API name
    };

    // Identify corners + record spline length leading up to each corner.
    let mut corners: Vec<InkTrapCorner> = Vec::new();
    let mut spline_length = 0.0_f64;
    let mut prev_direction = last_segment.direction_at(1.0);

    for (index, segment) in contour.segments.iter().enumerate() {
        let cur_direction = segment.direction_at(0.0);
        if is_corner(
            prev_direction.normalize(),
            cur_direction.normalize(),
            cross_threshold,
        ) {
            corners.push(InkTrapCorner {
                index,
                prev_edge_length_estimate: spline_length,
                minor: false,
                color: Color::BLACK,
            });
            spline_length = 0.0;
        }
        spline_length += estimate_edge_length(segment);
        prev_direction = segment.direction_at(1.0);
    }

    match corners.len() {
        0 => {
            // Smooth contour — single color for all segments.
            (color, seed) = color.switch(seed, Color::BLACK);
            let segments = contour
                .segments
                .iter()
                .map(|s| ColoredSegment { segment: *s, color }) // allow-banned: upstream fdsm API name
                .collect();
            (ColoredContour { segments }, color, seed) // allow-banned: upstream fdsm API name
        },
        1 => color_teardrop(contour, corners[0].index, color, seed),
        _ => color_multi_corner(contour, &mut corners, spline_length, color, seed),
    }
}

fn color_teardrop(
    contour: Contour, // allow-banned: upstream fdsm API name
    corner_idx: usize,
    color_in: Color,
    seed_in: u64,
) -> (ColoredContour, Color, u64) {
    let mut color = color_in;
    let mut seed = seed_in;

    // msdfgen: three colors — switched, WHITE, switched.
    (color, seed) = color.switch(seed, Color::BLACK);
    let color0 = color;
    let color1 = Color::WHITE;
    (color, seed) = color.switch(seed, Color::BLACK);
    let color2 = color;
    let color_sequence = [color0, color1, color2];

    let n = contour.segments.len();
    if n >= 3 {
        // Assign in-place via symmetrical trichotomy, starting at the corner.
        let mut segments_out: Vec<ColoredSegment> = contour // allow-banned: upstream fdsm API name
            .segments
            .iter()
            .map(|s| ColoredSegment {
                segment: *s,
                color:   Color::BLACK,
            }) // allow-banned: upstream fdsm API name
            .collect();
        for i in 0..n {
            let edge_index = (corner_idx + i) % n;
            segments_out[edge_index].color = color_sequence[trichotomy_color_index(i, n)];
        }
        return (
            ColoredContour {
                segments: segments_out,
            },
            color,
            seed,
        ); // allow-banned: upstream fdsm API name
    }

    // n in {1, 2}: split edge(s) into thirds. Layout in `parts[7]` mirrors
    // msdfgen's indexing where `corner` is the corner edge index (0 or 1).
    let mut parts: [Option<FdsmSegment>; 7] = [None; 7];
    let split0 = contour.segments[0].split_in_thirds();
    let base0 = 3 * corner_idx;
    parts[base0] = Some(split0[0]);
    parts[base0 + 1] = Some(split0[1]);
    parts[base0 + 2] = Some(split0[2]);
    if n >= 2 {
        let split1 = contour.segments[1].split_in_thirds();
        let base1 = if corner_idx == 0 { 3 } else { 0 };
        parts[base1] = Some(split1[0]);
        parts[base1 + 1] = Some(split1[1]);
        parts[base1 + 2] = Some(split1[2]);
    }

    // Color assignment (msdfgen lines 232-242):
    //   n >= 2: parts[0..2]=colors[0], parts[2..4]=colors[1], parts[4..6]=colors[2]
    //   n == 1: parts[0]=colors[0], parts[1]=colors[1], parts[2]=colors[2]
    let color_for_part: [Color; 7] = if n >= 2 {
        [
            color_sequence[0],
            color_sequence[0],
            color_sequence[1],
            color_sequence[1],
            color_sequence[2],
            color_sequence[2],
            Color::BLACK,
        ]
    } else {
        [
            color_sequence[0],
            color_sequence[1],
            color_sequence[2],
            Color::BLACK,
            Color::BLACK,
            Color::BLACK,
            Color::BLACK,
        ]
    };

    let mut segments_out: Vec<ColoredSegment> = Vec::new(); // allow-banned: upstream fdsm API name
    for (i, slot) in parts.iter().enumerate() {
        if let Some(seg) = slot {
            segments_out.push(ColoredSegment {
                segment: *seg,
                color:   color_for_part[i],
            }); // allow-banned: upstream fdsm API name
        } else {
            // msdfgen iterates `for (int i = 0; parts[i]; ++i)` — stops at
            // the first null. Mirror that early-stop semantics.
            break;
        }
    }
    (
        ColoredContour {
            segments: segments_out,
        },
        color,
        seed,
    ) // allow-banned: upstream fdsm API name
}

fn color_multi_corner(
    contour: Contour, // allow-banned: upstream fdsm API name
    corners: &mut [InkTrapCorner],
    contour_trailing_length: f64,
    color_in: Color,
    seed_in: u64,
) -> (ColoredContour, Color, u64) {
    let mut color = color_in;
    let mut seed = seed_in;
    let corner_count = corners.len();
    let mut major_corner_count = corner_count;

    if corner_count > 3 {
        // Wrap the loop's trailing spline length around to corners[0] so the
        // local-minimum test below covers the wraparound segment.
        corners[0].prev_edge_length_estimate += contour_trailing_length;
        for i in 0..corner_count {
            let prev_len = corners[i].prev_edge_length_estimate;
            let this_len = corners[(i + 1) % corner_count].prev_edge_length_estimate;
            let next_len = corners[(i + 2) % corner_count].prev_edge_length_estimate;
            if prev_len > this_len && this_len < next_len {
                corners[i].minor = true;
                major_corner_count -= 1;
            }
        }
    }

    // First pass: assign colors to MAJOR corners via the seeded rotation,
    // banning the initial color on the last major to close the loop.
    let mut initial_color = Color::BLACK;
    for corner in corners.iter_mut() {
        if !corner.minor {
            major_corner_count -= 1;
            let banned = if major_corner_count == 0 {
                initial_color
            } else {
                Color::BLACK
            };
            (color, seed) = color.switch(seed, banned);
            corner.color = color;
            if initial_color == Color::BLACK {
                initial_color = color;
            }
        }
    }

    // Second pass: derive colors for MINOR corners from `(color & next_color)
    // ^ WHITE`, where `color` tracks the most recent major corner's color.
    for i in 0..corner_count {
        if corners[i].minor {
            let next_color = corners[(i + 1) % corner_count].color;
            corners[i].color = (color & next_color) ^ Color::WHITE;
        } else {
            color = corners[i].color;
        }
    }

    // Walk the contour starting at the first corner, applying colors.
    let mut segments_out: Vec<ColoredSegment> = contour // allow-banned: upstream fdsm API name
        .segments
        .iter()
        .map(|s| ColoredSegment {
            segment: *s,
            color:   Color::BLACK,
        }) // allow-banned: upstream fdsm API name
        .collect();
    let m = contour.segments.len();
    let start = corners[0].index;
    let mut spline_idx = 0_usize;
    let mut current_color = corners[0].color;
    for i in 0..m {
        let index = (start + i) % m;
        if spline_idx + 1 < corner_count && corners[spline_idx + 1].index == index {
            spline_idx += 1;
            current_color = corners[spline_idx].color;
        }
        segments_out[index].color = current_color;
    }

    (
        ColoredContour {
            segments: segments_out,
        },
        color,
        seed,
    ) // allow-banned: upstream fdsm API name
}

fn is_corner(a_dir: Vector2<f64>, b_dir: Vector2<f64>, cross_threshold: f64) -> bool {
    let dot = a_dir.dot(&b_dir);
    let cross = a_dir.x.mul_add(b_dir.y, -(a_dir.y * b_dir.x));
    dot <= 0.0 || cross.abs() > cross_threshold
}

fn estimate_edge_length(segment: &FdsmSegment) -> f64 {
    let mut len = 0.0;
    let mut prev = segment.get(0.0);
    for i in 1..=EDGE_LENGTH_PRECISION {
        let t = f64::from(i) / f64::from(EDGE_LENGTH_PRECISION);
        let cur = segment.get(t);
        len += (cur - prev).norm();
        prev = cur;
    }
    len
}

fn trichotomy_color_index(position: usize, n: usize) -> usize {
    match symmetrical_trichotomy(position, n).clamp(-1, 1) {
        -1 => 0,
        0 => 1,
        _ => 2,
    }
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
        let colored = edge_coloring_ink_trap(outline, 3.0_f64.to_radians().sin(), 0);
        let mut out = Vec::new();
        for contour in &colored.contours {
            for seg in &contour.segments {
                out.push(seg.color.value());
            }
        }
        out
    }

    #[test]
    fn ink_trap_produces_non_empty_coloring_for_eb_garamond_g() {
        let masks = collect_masks(EB_GARAMOND, 'g');
        assert!(!masks.is_empty(), "expected at least one colored segment");
        for m in &masks {
            assert!(*m <= 7, "color mask out of range: {m}");
        }
    }

    #[test]
    fn ink_trap_handles_simple_glyphs() {
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
