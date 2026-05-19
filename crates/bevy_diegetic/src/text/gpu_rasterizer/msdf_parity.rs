//! MSDF parity test: Rust port of `shaders/msdf_gen.wgsl` compared
//! against the CPU `fdsm` MSDF pipeline (`MsdfRasterizer`).
//!
//! Mirrors the SDF parity infrastructure in `parity.rs` but for the
//! three-channel pipeline. The Rust port covers `msdf_gen` only —
//! `msdf_correct` and the truth-override safety net are intentionally
//! left out so the diff PNGs localize divergence in the raw
//! per-channel pick before error correction or sign reconciliation.
//!
//! Outputs five PNGs per glyph to `std::env::temp_dir()`:
//!
//! * `..._cpu.png`      — CPU `MsdfRasterizer` output (post-correction).
//! * `..._gpu_port.png` — Rust port of `msdf_gen.wgsl` (no correction).
//! * `..._diff.png`     — per-texel absolute median difference, red where GPU port > CPU, blue
//!   where < CPU.
//! * `..._channels_cpu.png`      — CPU R / G / B side by side.
//! * `..._channels_gpu_port.png` — GPU port R / G / B side by side.

#![cfg(test)]
#![allow(
    clippy::panic,
    clippy::unwrap_used,
    clippy::suboptimal_flops,
    clippy::many_single_char_names,
    reason = "tests use panic/unwrap for clearer failure messages; arithmetic is a verbatim mirror of WGSL kernel — mul_add would diverge from the GPU code under test"
)]

use std::path::Path;
use std::path::PathBuf;

use bevy_kana::ToF32;
use bevy_kana::ToU8;
use bevy_kana::ToU32;
use bevy_kana::ToUsize;
use fdsm::bezier::scanline::FillRule;
use fdsm::generate;
use fdsm::render;
use fdsm::shape::Shape; // allow-banned: upstream fdsm API name
use fdsm::transform::Transform;
use image::Rgb32FImage;
use nalgebra::Affine2;
use nalgebra::Matrix3;
use nalgebra::Point2;
use ttf_parser::Face;
use ttf_parser::GlyphId;

use super::edges;
use super::edges::EDGE_CHANNEL_MASK_BITS;
use super::edges::EDGE_CHANNEL_MASK_SHIFT;
use super::edges::EDGE_KIND_CUBIC;
use super::edges::EDGE_KIND_LINEAR;
use super::edges::EDGE_KIND_QUADRATIC;
use super::edges::EdgeSegment;
use crate::text::bitmap_dims;
use crate::text::constants::EDGE_COLORING_ANGLE;
use crate::text::constants::EDGE_COLORING_SEED;
use crate::text::msdf_rasterizer::DistanceField;
use crate::text::msdf_rasterizer::MsdfRasterizer;
use crate::text::msdf_rasterizer::RasterizedBitmap;
use crate::text::msdf_rasterizer::Rasterizer;

const JETBRAINS_MONO: &[u8] = include_bytes!("../../../assets/fonts/JetBrainsMono-Regular.ttf");
const EB_GARAMOND: &[u8] = include_bytes!("../../../assets/fonts/EBGaramond-Regular.ttf");

const CANONICAL_SIZE: u32 = 64;
const SDF_RANGE: f64 = 4.0;
const PADDING: u32 = 2;

fn glyph_index(font_data: &[u8], ch: char) -> u16 {
    let face = ttf_parser::Face::parse(font_data, 0).unwrap_or_else(|e| panic!("parse: {e}"));
    face.glyph_index(ch)
        .unwrap_or_else(|| panic!("no glyph for '{ch}'"))
        .0
}

#[derive(Clone, Copy)]
struct Vec2 {
    x: f32,
    y: f32,
}

impl Vec2 {
    const fn new(x: f32, y: f32) -> Self { Self { x, y } }

    fn sub(self, o: Self) -> Self { Self::new(self.x - o.x, self.y - o.y) }

    fn add(self, o: Self) -> Self { Self::new(self.x + o.x, self.y + o.y) }

    fn mul(self, s: f32) -> Self { Self::new(self.x * s, self.y * s) }

    fn dot(self, o: Self) -> f32 { self.x * o.x + self.y * o.y }

    fn length(self) -> f32 { self.dot(self).sqrt() }
}

const fn perp2(a: Vec2, b: Vec2) -> f32 { a.x * b.y - a.y * b.x }

const DEGENERATE_EPS: f32 = 1e-20;
const SQRT_3_OVER_2: f32 = 0.866_025_4;
const NEWTON_ITER: u32 = 4;
const CUBIC_SEEDS: &[f32] = &[0.0, 0.125, 0.25, 0.375, 0.5, 0.625, 0.75, 0.875, 1.0];
const INF_DIST: f32 = 1e30;

fn bezier_quadratic(t: f32, p0: Vec2, p1: Vec2, p2: Vec2) -> Vec2 {
    let one_minus = 1.0 - t;
    p0.mul(one_minus * one_minus)
        .add(p1.mul(2.0 * one_minus * t))
        .add(p2.mul(t * t))
}

fn bezier_quadratic_deriv(t: f32, p0: Vec2, p1: Vec2, p2: Vec2) -> Vec2 {
    p1.sub(p0).mul(2.0 * (1.0 - t)).add(p2.sub(p1).mul(2.0 * t))
}

fn bezier_cubic(t: f32, p0: Vec2, p1: Vec2, p2: Vec2, p3: Vec2) -> Vec2 {
    let one_minus = 1.0 - t;
    let mt2 = one_minus * one_minus;
    let t2 = t * t;
    p0.mul(mt2 * one_minus)
        .add(p1.mul(3.0 * mt2 * t))
        .add(p2.mul(3.0 * one_minus * t2))
        .add(p3.mul(t2 * t))
}

fn bezier_cubic_deriv(t: f32, p0: Vec2, p1: Vec2, p2: Vec2, p3: Vec2) -> Vec2 {
    let one_minus = 1.0 - t;
    p1.sub(p0)
        .mul(3.0 * one_minus * one_minus)
        .add(p2.sub(p1).mul(6.0 * one_minus * t))
        .add(p3.sub(p2).mul(3.0 * t * t))
}

fn bezier_cubic_deriv2(t: f32, p0: Vec2, p1: Vec2, p2: Vec2, p3: Vec2) -> Vec2 {
    let one_minus = 1.0 - t;
    let term0 = p2.sub(p1.mul(2.0)).add(p0);
    let term1 = p3.sub(p2.mul(2.0)).add(p1);
    term0.mul(6.0 * one_minus).add(term1.mul(6.0 * t))
}

fn cbrt_signed(x: f32) -> f32 { if x < 0.0 { -(-x).cbrt() } else { x.cbrt() } }

fn solve_cubic_normed(a: f32, b: f32, c: f32) -> ([f32; 3], u32) {
    let a2 = a * a;
    let q = (1.0 / 9.0) * (a2 - 3.0 * b);
    let r = (1.0 / 54.0) * (a * (2.0 * a2 - 9.0 * b) + 27.0 * c);
    let r2 = r * r;
    let q3 = q * q * q;
    let a_third = a * (1.0 / 3.0);
    if r2 < q3 {
        let t_norm = (r / q3.sqrt()).clamp(-1.0, 1.0);
        let theta = t_norm.acos();
        let q_pre = -2.0 * q.sqrt();
        let cos_t3 = (theta / 3.0).cos();
        let sin_t3 = (theta / 3.0).sin();
        let roots = [
            q_pre * cos_t3 - a_third,
            q_pre * (-0.5 * cos_t3 - SQRT_3_OVER_2 * sin_t3) - a_third,
            q_pre * (-0.5 * cos_t3 + SQRT_3_OVER_2 * sin_t3) - a_third,
        ];
        return (roots, 3);
    }
    let sgn = if r < 0.0 { 1.0 } else { -1.0 };
    let u = sgn * cbrt_signed(r.abs() + (r2 - q3).sqrt());
    let v = if u == 0.0 { 0.0 } else { q / u };
    ([(u + v) - a_third, 0.0, 0.0], 1)
}

#[derive(Clone, Copy)]
struct EdgeDist {
    dist_sq: f32,
    param:   f32,
    foot:    Vec2,
    tangent: Vec2,
}

fn distance_linear(pt: Vec2, p0: Vec2, p1: Vec2) -> EdgeDist {
    let d = p1.sub(p0);
    let len_sq = d.dot(d).max(DEGENERATE_EPS);
    let t_raw = pt.sub(p0).dot(d) / len_sq;
    let t_c = t_raw.clamp(0.0, 1.0);
    // When t_c clamps to an endpoint, use the stored endpoint directly so
    // sibling segments sharing a corner produce bit-exact equal foot/dist_sq —
    // `p0 + (p1-p0)·1.0` is not guaranteed equal to `p1` in f32.
    let foot = if t_c <= 0.0 {
        p0
    } else if t_c >= 1.0 {
        p1
    } else {
        p0.add(d.mul(t_c))
    };
    let diff = pt.sub(foot);
    EdgeDist {
        dist_sq: diff.dot(diff),
        param: t_raw,
        foot,
        tangent: d,
    }
}

fn distance_quadratic(pt: Vec2, p0: Vec2, p1: Vec2, p2: Vec2) -> EdgeDist {
    let pv = pt.sub(p0);
    let pv1 = p1.sub(p0);
    let pv2 = p2.sub(p1.mul(2.0)).add(p0);
    let mut best_sq = pv.dot(pv);
    let pv1_len_sq = pv1.dot(pv1).max(DEGENERATE_EPS);
    let mut best_t = pv.dot(pv1) / pv1_len_sq;

    let p2mo = p2.sub(pt);
    let d2 = p2mo.dot(p2mo);
    if d2 < best_sq {
        best_sq = d2;
        let ep_end = p2.sub(p1);
        let ep_len_sq = ep_end.dot(ep_end).max(DEGENERATE_EPS);
        best_t = pt.sub(p1).dot(ep_end) / ep_len_sq;
    }
    let a_norm_sq = pv2.dot(pv2);
    if a_norm_sq >= DEGENERATE_EPS {
        let ainv = a_norm_sq.recip();
        let (roots, n) = solve_cubic_normed(
            3.0 * pv1.dot(pv2) * ainv,
            (2.0 * pv1.dot(pv1) - pv2.dot(pv)) * ainv,
            -pv1.dot(pv) * ainv,
        );
        for &tr in roots.iter().take(n as usize) {
            if (0.0..=1.0).contains(&tr) {
                let q = p0.add(pv1.mul(2.0 * tr)).add(pv2.mul(tr * tr));
                let diff = q.sub(pt);
                let dsq = diff.dot(diff);
                if dsq < best_sq {
                    best_sq = dsq;
                    best_t = tr;
                }
            }
        }
    }
    let t_c = best_t.clamp(0.0, 1.0);
    let (foot, dist_sq) = if t_c <= 0.0 {
        let diff = pt.sub(p0);
        (p0, diff.dot(diff))
    } else if t_c >= 1.0 {
        let diff = pt.sub(p2);
        (p2, diff.dot(diff))
    } else {
        (bezier_quadratic(t_c, p0, p1, p2), best_sq)
    };
    let tangent = bezier_quadratic_deriv(t_c, p0, p1, p2);
    EdgeDist {
        dist_sq,
        param: best_t,
        foot,
        tangent,
    }
}

fn distance_cubic(pt: Vec2, p0: Vec2, p1: Vec2, p2: Vec2, p3: Vec2) -> EdgeDist {
    let pv = p0.sub(pt);
    let pv1 = p1.sub(p0);
    let pv3_end = p3.sub(p2);
    let pv1_len_sq = pv1.dot(pv1).max(DEGENERATE_EPS);
    let mut best_sq = pv.dot(pv);
    let mut best_t = -pv.dot(pv1) / pv1_len_sq;

    let p3mo = p3.sub(pt);
    let d2 = p3mo.dot(p3mo);
    if d2 < best_sq {
        best_sq = d2;
        let len_sq = pv3_end.dot(pv3_end).max(DEGENERATE_EPS);
        best_t = pt.sub(p2).dot(pv3_end) / len_sq;
    }

    for &seed in CUBIC_SEEDS {
        let mut t = seed;
        for _ in 0..NEWTON_ITER {
            let bt = bezier_cubic(t, p0, p1, p2, p3);
            let d1 = bezier_cubic_deriv(t, p0, p1, p2, p3);
            let d2v = bezier_cubic_deriv2(t, p0, p1, p2, p3);
            let qe = bt.sub(pt);
            let denom = d1.dot(d1) + qe.dot(d2v);
            if denom.abs() < DEGENERATE_EPS {
                break;
            }
            t -= qe.dot(d1) / denom;
            if t <= 0.0 || t >= 1.0 {
                break;
            }
            let bt2 = bezier_cubic(t, p0, p1, p2, p3);
            let diff = pt.sub(bt2);
            let dsq = diff.dot(diff);
            if dsq < best_sq {
                best_sq = dsq;
                best_t = t;
            }
        }
    }
    let t_c = best_t.clamp(0.0, 1.0);
    let (foot, dist_sq) = if t_c <= 0.0 {
        let diff = pt.sub(p0);
        (p0, diff.dot(diff))
    } else if t_c >= 1.0 {
        let diff = pt.sub(p3);
        (p3, diff.dot(diff))
    } else {
        (bezier_cubic(t_c, p0, p1, p2, p3), best_sq)
    };
    let tangent = bezier_cubic_deriv(t_c, p0, p1, p2, p3);
    EdgeDist {
        dist_sq,
        param: best_t,
        foot,
        tangent,
    }
}

fn signed_pseudo_distance(
    pt: Vec2,
    ed: EdgeDist,
    p_start: Vec2,
    p_end: Vec2,
    dir_start: Vec2,
    dir_end: Vec2,
) -> f32 {
    let unsigned_dist = ed.dist_sq.sqrt();
    let pmb = ed.foot.sub(pt);
    let pmb_len = pmb.length().max(DEGENERATE_EPS);
    let pmb_n = Vec2::new(pmb.x / pmb_len, pmb.y / pmb_len);
    let tan_len = ed.tangent.length().max(DEGENERATE_EPS);
    let tan_n = Vec2::new(ed.tangent.x / tan_len, ed.tangent.y / tan_len);
    let cross_main = perp2(tan_n, pmb_n);
    let main_sign = if cross_main >= 0.0 { 1.0 } else { -1.0 };
    let signed_main = unsigned_dist * main_sign;
    if ed.param < 0.0 {
        let dir_len = dir_start.length().max(DEGENERATE_EPS);
        let dir = Vec2::new(dir_start.x / dir_len, dir_start.y / dir_len);
        let aq = pt.sub(p_start);
        let ts = aq.dot(dir);
        if ts < 0.0 {
            let pseudo = perp2(aq, dir);
            if pseudo * pseudo <= ed.dist_sq {
                return pseudo;
            }
        }
    } else if ed.param > 1.0 {
        let dir_len = dir_end.length().max(DEGENERATE_EPS);
        let dir = Vec2::new(dir_end.x / dir_len, dir_end.y / dir_len);
        let bq = pt.sub(p_end);
        let ts = bq.dot(dir);
        if ts > 0.0 {
            let pseudo = perp2(bq, dir);
            if pseudo * pseudo <= ed.dist_sq {
                return pseudo;
            }
        }
    }
    signed_main
}

fn winding_linear(pt: Vec2, p0: Vec2, p1: Vec2) -> i32 {
    let dy = p1.y - p0.y;
    if dy.abs() < 1e-20 {
        return 0;
    }
    let t = (pt.y - p0.y) / dy;
    if !(0.0..1.0).contains(&t) {
        return 0;
    }
    let x = p0.x + t * (p1.x - p0.x);
    if x < pt.x {
        return 0;
    }
    if dy > 0.0 { 1 } else { -1 }
}

fn winding_subdivided(
    pt: Vec2,
    p0: Vec2,
    p_end: Vec2,
    eval: impl Fn(f32) -> Vec2,
    steps: u32,
) -> i32 {
    let mut acc = 0;
    let mut prev = p0;
    for i in 1..=steps {
        let t = i.to_f32() / steps.to_f32();
        let next = if i == steps { p_end } else { eval(t) };
        acc += winding_linear(pt, prev, next);
        prev = next;
    }
    acc
}

fn winding_quadratic(pt: Vec2, p0: Vec2, p1: Vec2, p2: Vec2) -> i32 {
    winding_subdivided(pt, p0, p2, |t| bezier_quadratic(t, p0, p1, p2), 8)
}

fn winding_cubic(pt: Vec2, p0: Vec2, p1: Vec2, p2: Vec2, p3: Vec2) -> i32 {
    winding_subdivided(pt, p0, p3, |t| bezier_cubic(t, p0, p1, p2, p3), 12)
}

fn pixel_winding(pt: Vec2, edges_buf: &[EdgeSegment]) -> i32 {
    let mut acc = 0;
    for e in edges_buf {
        let kind = e.kind & 0b11;
        let (p0, p1, p2, p3) = edge_points(e);
        if kind == EDGE_KIND_LINEAR {
            acc += winding_linear(pt, p0, p1);
        } else if kind == EDGE_KIND_QUADRATIC {
            acc += winding_quadratic(pt, p0, p1, p2);
        } else if kind == EDGE_KIND_CUBIC {
            acc += winding_cubic(pt, p0, p1, p2, p3);
        }
    }
    acc
}

fn edge_points(e: &EdgeSegment) -> (Vec2, Vec2, Vec2, Vec2) {
    (
        Vec2::new(e.points[0], e.points[1]),
        Vec2::new(e.points[2], e.points[3]),
        Vec2::new(e.points[4], e.points[5]),
        Vec2::new(e.points[6], e.points[7]),
    )
}

/// Per-texel rust port of `msdf_gen.wgsl`. Returns raw (R, G, B) signed
/// pseudo-distances scaled to 0..=255 — no error correction, no truth
/// override, no sign reconciliation across the three channels.
fn rust_port_msdf_pixel(pt: Vec2, edges_buf: &[EdgeSegment], sdf_range: f32) -> [u8; 3] {
    let mut best_sq = [INF_DIST; 3];
    let mut best_orth = [-1.0_f32; 3];
    let mut signed = [0.0_f32; 3];

    for e in edges_buf {
        let kind = e.kind & 0b11;
        let chan = (e.kind >> EDGE_CHANNEL_MASK_SHIFT) & EDGE_CHANNEL_MASK_BITS;
        let (p0, p1, p2, p3) = edge_points(e);

        let (ed, p_start, p_end, dir_start, dir_end) = if kind == EDGE_KIND_LINEAR {
            (distance_linear(pt, p0, p1), p0, p1, p1.sub(p0), p1.sub(p0))
        } else if kind == EDGE_KIND_QUADRATIC {
            (
                distance_quadratic(pt, p0, p1, p2),
                p0,
                p2,
                p1.sub(p0),
                p2.sub(p1),
            )
        } else if kind == EDGE_KIND_CUBIC {
            (
                distance_cubic(pt, p0, p1, p2, p3),
                p0,
                p3,
                p1.sub(p0),
                p3.sub(p2),
            )
        } else {
            continue;
        };

        let edge_signed = signed_pseudo_distance(pt, ed, p_start, p_end, dir_start, dir_end);

        let tan_len = ed.tangent.length().max(DEGENERATE_EPS);
        let tan_n = Vec2::new(ed.tangent.x / tan_len, ed.tangent.y / tan_len);
        let pmb = ed.foot.sub(pt);
        let pmb_len = pmb.length().max(DEGENERATE_EPS);
        let pmb_n = Vec2::new(pmb.x / pmb_len, pmb.y / pmb_len);
        let orth = perp2(tan_n, pmb_n).abs();

        for c in 0..3_usize {
            let bit = 1_u32 << c;
            if chan & bit == 0 {
                continue;
            }
            #[allow(clippy::float_cmp, reason = "exact equality matches WGSL behavior")]
            let take = ed.dist_sq < best_sq[c] || (ed.dist_sq == best_sq[c] && orth > best_orth[c]);
            if take {
                best_sq[c] = ed.dist_sq;
                best_orth[c] = orth;
                signed[c] = edge_signed;
            }
        }
    }

    let n0 = (signed[0] / sdf_range + 0.5).clamp(0.0, 1.0);
    let n1 = (signed[1] / sdf_range + 0.5).clamp(0.0, 1.0);
    let n2 = (signed[2] / sdf_range + 0.5).clamp(0.0, 1.0);

    // fdsm `correct_sign_msdf`: when median sign disagrees with the
    // non-zero winding rule, invert all three channels (`1.0 - n`).
    // Mirrored here so the parity output lines up with what CPU
    // `MsdfRasterizer` produces post-correction.
    let med = median3_f32(n0, n1, n2);
    let median_inside = med > 0.5;
    let truth_inside = pixel_winding(pt, edges_buf) != 0;
    let (o0, o1, o2) = if median_inside == truth_inside {
        (n0, n1, n2)
    } else {
        (1.0 - n0, 1.0 - n1, 1.0 - n2)
    };

    [
        (o0 * 255.0).to_u8(),
        (o1 * 255.0).to_u8(),
        (o2 * 255.0).to_u8(),
    ]
}

fn median3_f32(a: f32, b: f32, c: f32) -> f32 { a.min(b).max(a.max(b).min(c)) }

fn rust_port_msdf_bitmap(
    edges_buf: &[EdgeSegment],
    width: u32,
    height: u32,
    sdf_range: f32,
) -> Vec<u8> {
    let mut out = Vec::with_capacity((width * height * 3).to_usize());
    for y in 0..height {
        for x in 0..width {
            let pt = Vec2::new(x.to_f32() + 0.5, y.to_f32() + 0.5);
            let rgb = rust_port_msdf_pixel(pt, edges_buf, sdf_range);
            out.extend_from_slice(&rgb);
        }
    }
    out
}

fn cpu_msdf_bitmap(font_data: &[u8], ch: char) -> (Vec<u8>, u32, u32) {
    let r = MsdfRasterizer::new(CANONICAL_SIZE, SDF_RANGE, PADDING);
    let bitmap = r
        .rasterize(font_data, glyph_index(font_data, ch))
        .unwrap_or_else(|| panic!("CPU MSDF returned None for '{ch}'"));
    match bitmap {
        RasterizedBitmap::Msdf(b) => (b.data, b.width, b.height),
        RasterizedBitmap::Sdf(_) => panic!("expected MSDF variant"),
    }
}

/// Per-channel diagnostic: returns the raw fdsm signed pseudo
/// distances (f64) at every (x, y) in `texels`, plus their u8
/// encoding under our `SDF_RANGE`. Re-builds the prepared geometry from
/// scratch so the inputs identically match `cpu_msdf_no_error_bitmap`.
fn fdsm_signed_pseudo_at_texels(
    font_data: &[u8],
    ch: char,
    texels: &[(u32, u32)],
) -> Vec<([f64; 3], [u8; 3])> {
    let face = Face::parse(font_data, 0).unwrap_or_else(|e| panic!("parse: {e}"));
    let glyph_id = GlyphId(glyph_index(font_data, ch));
    let outline = fdsm_ttf_parser::load_shape_from_face(&face, glyph_id) // allow-banned: upstream fdsm API name
        .unwrap_or_else(|| panic!("no outline for '{ch}'"));
    let dims =
        bitmap_dims::compute_bitmap_size(&face, glyph_id, CANONICAL_SIZE, SDF_RANGE, PADDING)
            .unwrap_or_else(|| panic!("zero bitmap dims for '{ch}'"));
    let image_width = dims.width;
    let image_height = dims.height;
    let bbox = face.glyph_bounding_box(glyph_id).unwrap();
    let units_per_em = f64::from(face.units_per_em());
    let scale = f64::from(CANONICAL_SIZE) / units_per_em;
    let glyph_width = f64::from(bbox.x_max - bbox.x_min) * scale;
    let glyph_height = f64::from(bbox.y_max - bbox.y_min) * scale;
    let actual_pad_x = (f64::from(image_width) - glyph_width) / 2.0;
    let actual_pad_y = (f64::from(image_height) - glyph_height) / 2.0;
    let sin_alpha = EDGE_COLORING_ANGLE.to_radians().sin();
    let mut colored = Shape::edge_coloring_simple(outline, sin_alpha, EDGE_COLORING_SEED); // allow-banned: upstream fdsm API name
    let tx = actual_pad_x - f64::from(bbox.x_min) * scale;
    let ty = actual_pad_y + f64::from(bbox.y_max) * scale;
    let transform = Affine2::from_matrix_unchecked(Matrix3::new(
        scale, 0.0, tx, 0.0, -scale, ty, 0.0, 0.0, 1.0,
    ));
    colored.transform(&transform);
    let prepared = colored.prepare();
    let _ = image_height;

    texels
        .iter()
        .map(|&(x, y)| {
            let point = Point2::new(f64::from(x) + 0.5, f64::from(y) + 0.5);
            let [d_red, d_green, d_blue] = prepared.distance3(point);
            let dr = d_red.signed_pseudo_distance(point);
            let dg = d_green.signed_pseudo_distance(point);
            let db = d_blue.signed_pseudo_distance(point);
            (
                [dr, dg, db],
                [encode_msdf_u8(dr), encode_msdf_u8(dg), encode_msdf_u8(db)],
            )
        })
        .collect()
}

fn encode_msdf_u8(sd: f64) -> u8 {
    let n = (sd / SDF_RANGE + 0.5).clamp(0.0, 1.0);
    (n * 255.0).round().to_u32().to_u8()
}

#[derive(Clone, Copy)]
struct LinearPickF64 {
    chan:      u32,
    dist_sq:   f64,
    param:     f64,
    foot:      (f64, f64),
    tangent:   (f64, f64),
    p_start:   (f64, f64),
    p_end:     (f64, f64),
    dir_start: (f64, f64),
    dir_end:   (f64, f64),
}

fn linear_pick_f64(edge_segment: &EdgeSegment, pt_x: f64, pt_y: f64) -> Option<LinearPickF64> {
    let kind = edge_segment.kind & 0b11;
    if kind != EDGE_KIND_LINEAR {
        // For diagnostic simplicity, treat non-linear edges as not
        // contributing in this f64 mirror — it is only used to localize
        // where the picks diverge at the bad texels. If diagnostics show
        // bad texels with a non-linear nearest edge, this helper would need
        // the full quadratic / cubic ports too.
        return None;
    }

    let p = [
        (
            f64::from(edge_segment.points[0]),
            f64::from(edge_segment.points[1]),
        ),
        (
            f64::from(edge_segment.points[2]),
            f64::from(edge_segment.points[3]),
        ),
    ];
    let dx = p[1].0 - p[0].0;
    let dy = p[1].1 - p[0].1;
    let len_sq = (dx * dx + dy * dy).max(1e-20);
    let param = ((pt_x - p[0].0) * dx + (pt_y - p[0].1) * dy) / len_sq;
    let t_c = param.clamp(0.0, 1.0);
    let foot = if t_c <= 0.0 {
        p[0]
    } else if t_c >= 1.0 {
        p[1]
    } else {
        (p[0].0 + dx * t_c, p[0].1 + dy * t_c)
    };
    let ddx = pt_x - foot.0;
    let ddy = pt_y - foot.1;
    Some(LinearPickF64 {
        chan: (edge_segment.kind >> EDGE_CHANNEL_MASK_SHIFT) & EDGE_CHANNEL_MASK_BITS,
        dist_sq: ddx * ddx + ddy * ddy,
        param,
        foot,
        tangent: (dx, dy),
        p_start: p[0],
        p_end: p[1],
        dir_start: (dx, dy),
        dir_end: (dx, dy),
    })
}

fn signed_pseudo_distance_f64(pick: LinearPickF64, pt_x: f64, pt_y: f64) -> (f64, f64) {
    let tan_len = pick.tangent.0.hypot(pick.tangent.1).max(1e-20);
    let tan_n = (pick.tangent.0 / tan_len, pick.tangent.1 / tan_len);
    let pmb = (pick.foot.0 - pt_x, pick.foot.1 - pt_y);
    let pmb_len = pmb.0.hypot(pmb.1).max(1e-20);
    let pmb_n = (pmb.0 / pmb_len, pmb.1 / pmb_len);
    let cross = tan_n.0 * pmb_n.1 - tan_n.1 * pmb_n.0;
    let orth = cross.abs();
    let main_sign = if cross >= 0.0 { 1.0 } else { -1.0 };
    let mut edge_signed = pick.dist_sq.sqrt() * main_sign;
    if pick.param < 0.0 {
        edge_signed = start_pseudo_distance_f64(pick, pt_x, pt_y, edge_signed);
    } else if pick.param > 1.0 {
        edge_signed = end_pseudo_distance_f64(pick, pt_x, pt_y, edge_signed);
    }
    (orth, edge_signed)
}

fn start_pseudo_distance_f64(pick: LinearPickF64, pt_x: f64, pt_y: f64, edge_signed: f64) -> f64 {
    let dir_len = pick.dir_start.0.hypot(pick.dir_start.1).max(1e-20);
    let dir = (pick.dir_start.0 / dir_len, pick.dir_start.1 / dir_len);
    let aqx = pt_x - pick.p_start.0;
    let aqy = pt_y - pick.p_start.1;
    let ts = aqx * dir.0 + aqy * dir.1;
    if ts < 0.0 {
        let pseudo = aqx * dir.1 - aqy * dir.0;
        if pseudo * pseudo <= pick.dist_sq {
            return pseudo;
        }
    }
    edge_signed
}

fn end_pseudo_distance_f64(pick: LinearPickF64, pt_x: f64, pt_y: f64, edge_signed: f64) -> f64 {
    let dir_len = pick.dir_end.0.hypot(pick.dir_end.1).max(1e-20);
    let dir = (pick.dir_end.0 / dir_len, pick.dir_end.1 / dir_len);
    let bqx = pt_x - pick.p_end.0;
    let bqy = pt_y - pick.p_end.1;
    let ts = bqx * dir.0 + bqy * dir.1;
    if ts > 0.0 {
        let pseudo = bqx * dir.1 - bqy * dir.0;
        if pseudo * pseudo <= pick.dist_sq {
            return pseudo;
        }
    }
    edge_signed
}

fn update_f64_picks(
    pick: LinearPickF64,
    best_sq: &mut [f64; 3],
    best_orth: &mut [f64; 3],
    signed: &mut [f64; 3],
    orth: f64,
    edge_signed: f64,
) {
    for c in 0..3_usize {
        let bit = 1_u32 << c;
        if pick.chan & bit == 0 {
            continue;
        }
        #[allow(clippy::float_cmp, reason = "mirrors WGSL exact-equality tiebreaker")]
        let take = pick.dist_sq < best_sq[c] || (pick.dist_sq == best_sq[c] && orth > best_orth[c]);
        if take {
            best_sq[c] = pick.dist_sq;
            best_orth[c] = orth;
            signed[c] = edge_signed;
        }
    }
}

/// f64 mirror of `rust_port_msdf_pixel`'s gen-only path (no sign
/// reconciliation). Used by the diagnostic to test whether the
/// gen-only port matches fdsm in f64 — i.e., to separate the
/// "f32 vs f64 precision" question from the "different algorithm"
/// question.
fn rust_port_msdf_pixel_f64_gen(
    font_data: &[u8],
    ch: char,
    x: u32,
    y: u32,
    edges_buf: &[EdgeSegment],
) -> ([f64; 3], [u8; 3]) {
    let _ = (font_data, ch);
    let pt_x = f64::from(x) + 0.5;
    let pt_y = f64::from(y) + 0.5;

    let mut best_sq = [f64::INFINITY; 3];
    let mut best_orth = [-1.0_f64; 3];
    let mut signed = [0.0_f64; 3];

    for edge_segment in edges_buf {
        let Some(pick) = linear_pick_f64(edge_segment, pt_x, pt_y) else {
            continue;
        };
        let (orth, edge_signed) = signed_pseudo_distance_f64(pick, pt_x, pt_y);
        update_f64_picks(
            pick,
            &mut best_sq,
            &mut best_orth,
            &mut signed,
            orth,
            edge_signed,
        );
    }
    (
        signed,
        [
            encode_msdf_u8(signed[0]),
            encode_msdf_u8(signed[1]),
            encode_msdf_u8(signed[2]),
        ],
    )
}

/// Runs only fdsm's `generate_msdf` + `correct_sign_msdf` — skips
/// `correct_error_msdf`. Used by the parity test to isolate the exact
/// texels error correction modifies on CPU.
fn cpu_msdf_no_error_bitmap(font_data: &[u8], ch: char) -> (Vec<u8>, u32, u32) {
    let face = Face::parse(font_data, 0).unwrap_or_else(|e| panic!("parse: {e}"));
    let glyph_id = GlyphId(glyph_index(font_data, ch));
    let outline = fdsm_ttf_parser::load_shape_from_face(&face, glyph_id) // allow-banned: upstream fdsm API name
        .unwrap_or_else(|| panic!("no outline for '{ch}'"));

    let dims =
        bitmap_dims::compute_bitmap_size(&face, glyph_id, CANONICAL_SIZE, SDF_RANGE, PADDING)
            .unwrap_or_else(|| panic!("zero bitmap dims for '{ch}'"));
    let image_width = dims.width;
    let image_height = dims.height;

    let bbox = face
        .glyph_bounding_box(glyph_id)
        .unwrap_or_else(|| panic!("no bbox for '{ch}'"));
    let units_per_em = f64::from(face.units_per_em());
    let scale = f64::from(CANONICAL_SIZE) / units_per_em;
    let glyph_width = f64::from(bbox.x_max - bbox.x_min) * scale;
    let glyph_height = f64::from(bbox.y_max - bbox.y_min) * scale;
    let actual_pad_x = (f64::from(image_width) - glyph_width) / 2.0;
    let actual_pad_y = (f64::from(image_height) - glyph_height) / 2.0;

    let sin_alpha = EDGE_COLORING_ANGLE.to_radians().sin();
    let mut colored = Shape::edge_coloring_simple(outline, sin_alpha, EDGE_COLORING_SEED); // allow-banned: upstream fdsm API name

    let tx = actual_pad_x - f64::from(bbox.x_min) * scale;
    let ty = actual_pad_y + f64::from(bbox.y_max) * scale;
    let transform = Affine2::from_matrix_unchecked(Matrix3::new(
        scale, 0.0, tx, 0.0, -scale, ty, 0.0, 0.0, 1.0,
    ));
    colored.transform(&transform);
    let prepared = colored.prepare();

    let mut image_f32 = Rgb32FImage::new(image_width, image_height);
    generate::generate_msdf(&prepared, SDF_RANGE, &mut image_f32);
    render::correct_sign_msdf(&mut image_f32, &prepared, FillRule::Nonzero);
    // `correct_error_msdf` intentionally skipped.

    let total = (image_width * image_height * 3).to_usize();
    let mut data = Vec::with_capacity(total);
    for y in 0..image_height {
        for x in 0..image_width {
            let p = image_f32.get_pixel(x, y);
            data.push((p[0].clamp(0.0, 1.0) * 255.0).to_u8());
            data.push((p[1].clamp(0.0, 1.0) * 255.0).to_u8());
            data.push((p[2].clamp(0.0, 1.0) * 255.0).to_u8());
        }
    }
    (data, image_width, image_height)
}

fn median3(a: u8, b: u8, c: u8) -> u8 {
    let mut v = [a, b, c];
    v.sort_unstable();
    v[1]
}

fn save_png(path: &Path, width: u32, height: u32, rgb: Vec<u8>) {
    let Some(img) = image::RgbImage::from_raw(width, height, rgb) else {
        eprintln!("[msdf parity] could not build image for {}", path.display());
        return;
    };
    if let Err(e) = img.save(path) {
        eprintln!("[msdf parity] could not save {}: {e}", path.display());
    }
}

fn upscale_rgb(rgb: &[u8], width: u32, height: u32, scale: u32) -> Vec<u8> {
    let up_w = width * scale;
    let up_h = height * scale;
    let mut out = Vec::with_capacity((up_w * up_h * 3).to_usize());
    for y in 0..up_h {
        for x in 0..up_w {
            let sx = x / scale;
            let sy = y / scale;
            let idx = ((sy * width + sx) * 3).to_usize();
            out.extend_from_slice(&rgb[idx..idx + 3]);
        }
    }
    out
}

fn channels_strip(rgb: &[u8], width: u32, height: u32) -> Vec<u8> {
    // Stack R, G, B as three grayscale tiles side by side, with a 2-px
    // black separator. Output width = 3 * width + 4.
    let sep = 2_u32;
    let out_w = width * 3 + sep * 2;
    let mut out = vec![0_u8; (out_w * height * 3).to_usize()];
    for y in 0..height {
        for x in 0..width {
            let in_idx = ((y * width + x) * 3).to_usize();
            let r = rgb[in_idx];
            let g = rgb[in_idx + 1];
            let b = rgb[in_idx + 2];
            let row = (y * out_w * 3).to_usize();
            // R tile.
            let r_idx = row + (x * 3).to_usize();
            out[r_idx] = r;
            out[r_idx + 1] = r;
            out[r_idx + 2] = r;
            // G tile.
            let g_idx = row + ((width + sep + x) * 3).to_usize();
            out[g_idx] = g;
            out[g_idx + 1] = g;
            out[g_idx + 2] = g;
            // B tile.
            let b_idx = row + ((width * 2 + sep * 2 + x) * 3).to_usize();
            out[b_idx] = b;
            out[b_idx + 1] = b;
            out[b_idx + 2] = b;
        }
    }
    out
}

struct BadTexel {
    x:       u32,
    y:       u32,
    cpu:     [u8; 3],
    gpu:     [u8; 3],
    cpu_med: u8,
    gpu_med: u8,
    diff:    i32,
}

fn diff_image(
    cpu: &[u8],
    gpu: &[u8],
    width: u32,
    height: u32,
) -> (Vec<u8>, i32, usize, Vec<BadTexel>) {
    let total = (width * height).to_usize();
    let mut out = Vec::with_capacity(total * 3);
    let mut max_diff = 0_i32;
    let mut bad = 0_usize;
    let mut bad_list: Vec<BadTexel> = Vec::new();
    for i in 0..total {
        let ci = i * 3;
        let cpu_med = median3(cpu[ci], cpu[ci + 1], cpu[ci + 2]);
        let gpu_med = median3(gpu[ci], gpu[ci + 1], gpu[ci + 2]);
        let d = i32::from(gpu_med) - i32::from(cpu_med);
        if d.abs() > max_diff {
            max_diff = d.abs();
        }
        if d.abs() > 16 {
            bad += 1;
            let xi = (i.to_u32()) % width;
            let yi = (i.to_u32()) / width;
            bad_list.push(BadTexel {
                x: xi,
                y: yi,
                cpu: [cpu[ci], cpu[ci + 1], cpu[ci + 2]],
                gpu: [gpu[ci], gpu[ci + 1], gpu[ci + 2]],
                cpu_med,
                gpu_med,
                diff: d,
            });
        }
        if d.abs() <= 4 {
            let v = cpu_med / 2;
            out.extend_from_slice(&[v, v, v]);
        } else if d > 0 {
            out.extend_from_slice(&[u8::min(255, d.unsigned_abs().to_u8()), 0, 0]);
        } else {
            out.extend_from_slice(&[0, 0, u8::min(255, d.unsigned_abs().to_u8())]);
        }
    }
    (out, max_diff, bad, bad_list)
}

struct ParityOutputPaths {
    cpu:      PathBuf,
    gpu_port: PathBuf,
    diff:     PathBuf,
    cpu_chan: PathBuf,
    gpu_chan: PathBuf,
}

impl ParityOutputPaths {
    fn new(font_label: &str, ch: char) -> Self {
        let safe_label: String = font_label
            .chars()
            .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
            .collect();
        let tmp = std::env::temp_dir();
        Self {
            cpu:      tmp.join(format!(
                "bevy_diegetic_msdf_parity_{safe_label}_{ch}_cpu.png"
            )),
            gpu_port: tmp.join(format!(
                "bevy_diegetic_msdf_parity_{safe_label}_{ch}_gpu_port.png"
            )),
            diff:     tmp.join(format!(
                "bevy_diegetic_msdf_parity_{safe_label}_{ch}_diff.png"
            )),
            cpu_chan: tmp.join(format!(
                "bevy_diegetic_msdf_parity_{safe_label}_{ch}_channels_cpu.png"
            )),
            gpu_chan: tmp.join(format!(
                "bevy_diegetic_msdf_parity_{safe_label}_{ch}_channels_gpu_port.png"
            )),
        }
    }
}

fn save_parity_images(
    paths: &ParityOutputPaths,
    cpu: &[u8],
    gpu_port: &[u8],
    diff: &[u8],
    width: u32,
    height: u32,
    scale: u32,
) {
    let up_w = width * scale;
    let up_h = height * scale;
    save_png(
        paths.cpu.as_path(),
        up_w,
        up_h,
        upscale_rgb(cpu, width, height, scale),
    );
    save_png(
        paths.gpu_port.as_path(),
        up_w,
        up_h,
        upscale_rgb(gpu_port, width, height, scale),
    );
    save_png(
        paths.diff.as_path(),
        up_w,
        up_h,
        upscale_rgb(diff, width, height, scale),
    );

    let cpu_strip = channels_strip(cpu, width, height);
    let gpu_strip = channels_strip(gpu_port, width, height);
    let strip_w = width * 3 + 4;
    save_png(
        paths.cpu_chan.as_path(),
        strip_w * scale,
        height * scale,
        upscale_rgb(&cpu_strip, strip_w, height, scale),
    );
    save_png(
        paths.gpu_chan.as_path(),
        strip_w * scale,
        height * scale,
        upscale_rgb(&gpu_strip, strip_w, height, scale),
    );
}

fn report_parity_summary(
    font_label: &str,
    ch: char,
    width: u32,
    height: u32,
    max_diff: i32,
    bad: usize,
    paths: &ParityOutputPaths,
) {
    let total = (width * height).to_usize();
    let bad_fraction = bad.to_f32() / total.to_f32();
    eprintln!(
        "[msdf parity] {font_label} '{ch}': {width}x{height}, max median diff {max_diff}, \
         {:.2}% texels diff > 16 ({bad}/{total})",
        bad_fraction * 100.0
    );
    eprintln!("  cpu       → {}", paths.cpu.display());
    eprintln!("  gpu_port  → {}", paths.gpu_port.display());
    eprintln!("  diff      → {}", paths.diff.display());
    eprintln!("  cpu chans → {}", paths.cpu_chan.display());
    eprintln!("  gpu chans → {}", paths.gpu_chan.display());
}

fn report_no_error_diffs(max_no_err: i32, bad_no_err: usize, no_err_list: &[BadTexel]) {
    eprintln!(
        "  vs cpu_no_error (gen + correct_sign only): max median diff {max_no_err}, \
         {bad_no_err} bad (|>16|)"
    );
    for t in no_err_list {
        eprintln!(
            "    [no-err diff] ({:>3},{:>3}) cpu_no_err={:>3},{:>3},{:>3} | \
             gpu_port={:>3},{:>3},{:>3} | diff={:+4}",
            t.x, t.y, t.cpu[0], t.cpu[1], t.cpu[2], t.gpu[0], t.gpu[1], t.gpu[2], t.diff,
        );
    }
}

fn report_bad_texels(
    font_data: &[u8],
    ch: char,
    edges_buf: &[EdgeSegment],
    cpu: &[u8],
    width: u32,
    height: u32,
    bad_list: &[BadTexel],
) {
    let total = (width * height).to_usize();
    eprintln!(
        "  bad texels (|median diff| > 16): {} of {}",
        bad_list.len(),
        total
    );
    let coords: Vec<(u32, u32)> = bad_list.iter().map(|t| (t.x, t.y)).collect();
    let fdsm_raw = fdsm_signed_pseudo_at_texels(font_data, ch, &coords);
    if let Some(first) = bad_list.first() {
        dump_per_edge_picks(edges_buf, first.x, first.y);
    }
    for (bad_texel, (sd, enc)) in bad_list.iter().zip(fdsm_raw.iter()) {
        report_bad_texel_detail(
            font_data, ch, edges_buf, cpu, width, height, bad_texel, sd, *enc,
        );
    }
}

fn report_bad_texel_detail(
    font_data: &[u8],
    ch: char,
    edges_buf: &[EdgeSegment],
    cpu: &[u8],
    width: u32,
    height: u32,
    bad_texel: &BadTexel,
    sd: &[f64; 3],
    enc: [u8; 3],
) {
    let cls = classify_bad_texel(bad_texel.x, bad_texel.y, width, height, cpu);
    eprintln!(
        "    ({:>3},{:>3}) cpu={:>3},{:>3},{:>3} med={:>3} | gpu_port={:>3},{:>3},{:>3} \
         med={:>3} | diff={:+4} | neighborhood: {}",
        bad_texel.x,
        bad_texel.y,
        bad_texel.cpu[0],
        bad_texel.cpu[1],
        bad_texel.cpu[2],
        bad_texel.cpu_med,
        bad_texel.gpu[0],
        bad_texel.gpu[1],
        bad_texel.gpu[2],
        bad_texel.gpu_med,
        bad_texel.diff,
        cls,
    );
    eprintln!(
        "         fdsm raw signed_pseudo: R={:+.4} ({}) G={:+.4} ({}) B={:+.4} ({})",
        sd[0], enc[0], sd[1], enc[1], sd[2], enc[2]
    );
    let (sd_f64, enc_f64) =
        rust_port_msdf_pixel_f64_gen(font_data, ch, bad_texel.x, bad_texel.y, edges_buf);
    eprintln!(
        "         port f64 (linears):    R={:+.4} ({}) G={:+.4} ({}) B={:+.4} ({})",
        sd_f64[0], enc_f64[0], sd_f64[1], enc_f64[1], sd_f64[2], enc_f64[2]
    );
}

fn write_parity_outputs(font_data: &[u8], ch: char, font_label: &str) {
    let idx = glyph_index(font_data, ch);
    let Some(body) = edges::build_edge_buffer(
        font_data,
        idx,
        CANONICAL_SIZE,
        SDF_RANGE,
        PADDING,
        DistanceField::Msdf,
    ) else {
        panic!("build_edge_buffer returned None for '{ch}'");
    };

    let (cpu, cpu_w, cpu_h) = cpu_msdf_bitmap(font_data, ch);
    assert_eq!(
        (body.bitmap_size.x, body.bitmap_size.y),
        (cpu_w, cpu_h),
        "{font_label} '{ch}': bitmap dims disagree between CPU and edge builder"
    );

    let (cpu_no_err, _, _) = cpu_msdf_no_error_bitmap(font_data, ch);

    let gpu_port = rust_port_msdf_bitmap(&body.edges, cpu_w, cpu_h, SDF_RANGE.to_f32());
    assert_eq!(
        cpu.len(),
        gpu_port.len(),
        "{font_label} '{ch}': cpu vs gpu_port length mismatch"
    );

    let scale = 4_u32;
    let paths = ParityOutputPaths::new(font_label, ch);
    let (diff, max_diff, bad, bad_list) = diff_image(&cpu, &gpu_port, cpu_w, cpu_h);
    let (_, max_no_err, bad_no_err, no_err_list) = diff_image(&cpu_no_err, &gpu_port, cpu_w, cpu_h);

    save_parity_images(&paths, &cpu, &gpu_port, &diff, cpu_w, cpu_h, scale);
    report_parity_summary(font_label, ch, cpu_w, cpu_h, max_diff, bad, &paths);
    report_no_error_diffs(max_no_err, bad_no_err, &no_err_list);

    if !bad_list.is_empty() {
        report_bad_texels(font_data, ch, &body.edges, &cpu, cpu_w, cpu_h, &bad_list);
    }
}

/// Coarse spatial classification of a divergent texel: look at the 3x3
/// neighborhood of CPU medians to decide whether the texel sits on a
/// glyph corner / edge / interior / padding. Helps tell whether
/// fdsm-vs-port disagreements cluster at corners (the rounded-corner
/// signal) or elsewhere.
fn classify_bad_texel(x: u32, y: u32, w: u32, h: u32, cpu: &[u8]) -> &'static str {
    let xi = i64::from(x);
    let yi = i64::from(y);
    let wi = i64::from(w);
    let hi = i64::from(h);
    let mut above = 0_u32;
    let mut below = 0_u32;
    let mut total = 0_u32;
    for dy in -1..=1_i64 {
        for dx in -1..=1_i64 {
            if dx == 0 && dy == 0 {
                continue;
            }
            let nx = xi + dx;
            let ny = yi + dy;
            if nx < 0 || ny < 0 || nx >= wi || ny >= hi {
                continue;
            }
            let idx = usize::try_from((ny * wi + nx) * 3).unwrap();
            let med = median3(cpu[idx], cpu[idx + 1], cpu[idx + 2]);
            total += 1;
            if med > 127 {
                above += 1;
            } else {
                below += 1;
            }
        }
    }
    if total < 5 {
        return "border";
    }
    // If almost all neighbors are inside or almost all are outside, the
    // texel sits on an interior/exterior plateau (or padding). If the
    // split is mixed (3:5, 4:4, etc.) the texel sits on the glyph
    // boundary; a near-even mix with both sides represented suggests a
    // corner / sharp turn rather than a smooth edge.
    let min_side = above.min(below);
    match min_side {
        0 => {
            if above == total {
                "interior plateau"
            } else {
                "exterior / padding"
            }
        },
        1 => "edge (gentle)",
        2 => "edge",
        _ => "corner / sharp",
    }
}

/// Walks every edge in `edges_buf` and prints its kind, channel mask,
/// `dist_sq` (f32), orthogonality, and signed pseudo-distance at
/// `(x, y)`. Used to localize which edge each per-channel pick latches
/// onto and how the f32 numerics interact with the orth tiebreaker.
fn dump_per_edge_picks(edges_buf: &[EdgeSegment], x: u32, y: u32) {
    let pt = Vec2::new(x.to_f32() + 0.5, y.to_f32() + 0.5);
    eprintln!("  per-edge dump @ ({x},{y}):");
    for (i, e) in edges_buf.iter().enumerate() {
        let kind = e.kind & 0b11;
        let mask = (e.kind >> EDGE_CHANNEL_MASK_SHIFT) & EDGE_CHANNEL_MASK_BITS;
        let (p0, p1, p2, p3) = (
            Vec2::new(e.points[0], e.points[1]),
            Vec2::new(e.points[2], e.points[3]),
            Vec2::new(e.points[4], e.points[5]),
            Vec2::new(e.points[6], e.points[7]),
        );
        let (ed, p_start, p_end, dir_start, dir_end, kname) = if kind == EDGE_KIND_LINEAR {
            (
                distance_linear(pt, p0, p1),
                p0,
                p1,
                p1.sub(p0),
                p1.sub(p0),
                "lin",
            )
        } else if kind == EDGE_KIND_QUADRATIC {
            (
                distance_quadratic(pt, p0, p1, p2),
                p0,
                p2,
                p1.sub(p0),
                p2.sub(p1),
                "quad",
            )
        } else if kind == EDGE_KIND_CUBIC {
            (
                distance_cubic(pt, p0, p1, p2, p3),
                p0,
                p3,
                p1.sub(p0),
                p3.sub(p2),
                "cube",
            )
        } else {
            continue;
        };
        let edge_signed = signed_pseudo_distance(pt, ed, p_start, p_end, dir_start, dir_end);
        let tan_len = ed.tangent.length().max(DEGENERATE_EPS);
        let tan_n = Vec2::new(ed.tangent.x / tan_len, ed.tangent.y / tan_len);
        let pmb = ed.foot.sub(pt);
        let pmb_len = pmb.length().max(DEGENERATE_EPS);
        let pmb_n = Vec2::new(pmb.x / pmb_len, pmb.y / pmb_len);
        let orth = perp2(tan_n, pmb_n).abs();
        let r = if mask & 1 != 0 { 'R' } else { '.' };
        let g = if mask & 2 != 0 { 'G' } else { '.' };
        let b = if mask & 4 != 0 { 'B' } else { '.' };
        eprintln!(
            "    [{:>3}] {} mask={:03b}({r}{g}{b}) param={:+.4} dist={:+.4} orth={:.4} \
             signed_pseudo={:+.4} | p0=({:+.2},{:+.2}) p1=({:+.2},{:+.2}) tangent=({:+.3},{:+.3})",
            i,
            kname,
            mask,
            ed.param,
            ed.dist_sq.sqrt(),
            orth,
            edge_signed,
            p0.x,
            p0.y,
            p1.x,
            p1.y,
            tan_n.x,
            tan_n.y,
        );
    }
}

#[test]
fn msdf_parity_jbm_h() { write_parity_outputs(JETBRAINS_MONO, 'h', "JetBrains Mono"); }

#[test]
fn msdf_parity_jbm_a() { write_parity_outputs(JETBRAINS_MONO, 'A', "JetBrains Mono"); }

#[test]
fn msdf_parity_jbm_o() { write_parity_outputs(JETBRAINS_MONO, 'O', "JetBrains Mono"); }

#[test]
fn msdf_parity_ebg_v() { write_parity_outputs(EB_GARAMOND, 'V', "EB Garamond"); }

#[test]
fn msdf_parity_ebg_h() { write_parity_outputs(EB_GARAMOND, 'h', "EB Garamond"); }

/// MTSDF reuses the MSDF generation path one-for-one — the channel
/// coloring, the corner list, every per-edge payload. The only delta
/// is the GPU correction kernel's alpha write, which has no CPU
/// counterpart. This test catches future drift between the two arms
/// of the `match` in `edges::build_edge_buffer` and the parity that
/// the fragment shader's MTSDF clamp depends on.
fn assert_edge_buffer_matches_msdf(font_data: &[u8], ch: char, font_label: &str) {
    let idx = glyph_index(font_data, ch);
    let msdf = edges::build_edge_buffer(
        font_data,
        idx,
        CANONICAL_SIZE,
        SDF_RANGE,
        PADDING,
        DistanceField::Msdf,
    )
    .unwrap_or_else(|| panic!("MSDF edge buffer None for {font_label} '{ch}'"));
    let mtsdf = edges::build_edge_buffer(
        font_data,
        idx,
        CANONICAL_SIZE,
        SDF_RANGE,
        PADDING,
        DistanceField::Mtsdf,
    )
    .unwrap_or_else(|| panic!("MTSDF edge buffer None for {font_label} '{ch}'"));

    assert_eq!(
        msdf.bitmap_size, mtsdf.bitmap_size,
        "{font_label} '{ch}': bitmap dims diverge between MSDF and MTSDF"
    );
    assert_eq!(
        msdf.edges.len(),
        mtsdf.edges.len(),
        "{font_label} '{ch}': edge count diverges"
    );
    assert_eq!(
        msdf.corners.len(),
        mtsdf.corners.len(),
        "{font_label} '{ch}': corner count diverges"
    );
    for (i, (a, b)) in msdf.edges.iter().zip(&mtsdf.edges).enumerate() {
        assert_eq!(
            a.kind, b.kind,
            "{font_label} '{ch}': edge[{i}] kind diverges ({} vs {})",
            a.kind, b.kind
        );
        assert_eq!(
            a.points, b.points,
            "{font_label} '{ch}': edge[{i}] points diverge"
        );
    }
}

#[test]
fn mtsdf_edge_buffer_matches_msdf_jbm_h() {
    assert_edge_buffer_matches_msdf(JETBRAINS_MONO, 'h', "JetBrains Mono");
}

#[test]
fn mtsdf_edge_buffer_matches_msdf_ebg_g() {
    assert_edge_buffer_matches_msdf(EB_GARAMOND, 'g', "EB Garamond");
}
