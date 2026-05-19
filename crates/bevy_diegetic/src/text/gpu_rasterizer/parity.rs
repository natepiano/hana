//! Parity test: Rust port of the WGSL SDF kernel compared against the
//! CPU `fdsm` SDF path.
//!
//! The Rust port mirrors `shaders/sdf_gen.wgsl` line-for-line so that
//! any algorithm bug (wrong sign convention, off-by-one, wrong
//! normalization) shows up the same way it would on the GPU. This
//! catches the high-value class of WGSL bugs without spinning up a
//! wgpu device in a `cargo test` run.
//!
//! A headless wgpu integration test that compiles the actual shader
//! and reads back a rendered texture is a separate follow-up; this
//! file covers the math.

#![cfg(test)]
#![allow(
    clippy::panic,
    clippy::unwrap_used,
    clippy::suboptimal_flops,
    reason = "tests use panic/unwrap for clearer failure messages; arithmetic is a verbatim mirror of WGSL kernel — mul_add would diverge from the GPU code under test"
)]

use bevy_kana::ToF32;
use bevy_kana::ToU8;
use bevy_kana::ToUsize;

use super::edges;
use super::edges::EDGE_KIND_CUBIC;
use super::edges::EDGE_KIND_LINEAR;
use super::edges::EDGE_KIND_QUADRATIC;
use super::edges::EdgeSegment;
use crate::text::msdf_rasterizer::DistanceField;
use crate::text::msdf_rasterizer::RasterizedBitmap;
use crate::text::msdf_rasterizer::Rasterizer;
use crate::text::msdf_rasterizer::SdfRasterizer;

const JETBRAINS_MONO: &[u8] = include_bytes!("../../../assets/fonts/JetBrainsMono-Regular.ttf");
const EB_GARAMOND: &[u8] = include_bytes!("../../../assets/fonts/EBGaramond-Regular.ttf");

const CANONICAL_SIZE: u32 = 64;
const SDF_RANGE: f64 = 4.0;
const PADDING: u32 = 2;
/// Per-texel tolerance in 0..=255 distance-value units. CPU fdsm uses
/// a scanline fill for sign correction while the Rust port (and WGSL
/// kernel) use a per-pixel ray cast. The two algorithms produce
/// identical signed-distance values in the interior of solid regions
/// but disagree at boundary pixels by up to ~1 px of signed distance
/// (~64 units for the default `SDF_RANGE` = 4), forming a thin halo
/// along the outline. The tolerance below ignores those boundary
/// pixels; the cap on `MAX_BAD_FRACTION` ensures the halo stays
/// thin.
const TOLERANCE: i32 = 64;
/// Allowable fraction of pixels exceeding `TOLERANCE`. At canonical
/// size 64 a glyph's outline is roughly 4 × outer perimeter texels
/// (one halo texel on each side of the outline, doubled for
/// glyphs with counters like 'O'); a 64x64 bitmap is 4096 texels, so
/// up to ~5% halo coverage is normal. The threshold below catches
/// systematic algorithm bugs (sign-flipped interior, fully-broken
/// kernel) while accepting the per-edge halo.
const MAX_BAD_FRACTION: f64 = 0.08;

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

fn bezier_quadratic(t: f32, p0: Vec2, p1: Vec2, p2: Vec2) -> Vec2 {
    let one_minus = 1.0 - t;
    p0.mul(one_minus * one_minus)
        .add(p1.mul(2.0 * one_minus * t))
        .add(p2.mul(t * t))
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

const SQRT_3_OVER_2: f32 = 0.866_025_4;

fn cbrt_signed(x: f32) -> f32 { if x < 0.0 { -(-x).cbrt() } else { x.cbrt() } }

/// Solves x³ + a x² + b x + c = 0. Returns `(roots, count)`.
#[allow(
    clippy::many_single_char_names,
    reason = "matches standard cubic-solver notation; renaming hurts readability of the math"
)]
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

fn distance_linear(pt: Vec2, p0: Vec2, p1: Vec2) -> f32 {
    let d = p1.sub(p0);
    let len_sq = d.dot(d).max(1e-20);
    let t = (pt.sub(p0).dot(d) / len_sq).clamp(0.0, 1.0);
    pt.sub(p0.add(d.mul(t))).length()
}

const DEGENERATE_EPS: f32 = 1e-20;
const NEWTON_ITER: u32 = 4;
const CUBIC_SEEDS: &[f32] = &[0.0, 0.125, 0.25, 0.375, 0.5, 0.625, 0.75, 0.875, 1.0];

fn distance_quadratic(pt: Vec2, p0: Vec2, p1: Vec2, p2: Vec2) -> f32 {
    let pv = pt.sub(p0);
    let pv1 = p1.sub(p0);
    let pv2 = p2.sub(p1.mul(2.0)).add(p0);
    let a_norm_sq = pv2.dot(pv2);

    let dp0 = pv;
    let dp2 = p2.sub(pt);
    let mut best_sq = dp0.dot(dp0).min(dp2.dot(dp2));

    if a_norm_sq < DEGENERATE_EPS {
        return best_sq.sqrt().min(distance_linear(pt, p0, p2));
    }

    let ainv = a_norm_sq.recip();
    let (roots, n) = solve_cubic_normed(
        3.0 * pv1.dot(pv2) * ainv,
        (2.0 * pv1.dot(pv1) - pv2.dot(pv)) * ainv,
        -pv1.dot(pv) * ainv,
    );
    for &t in roots.iter().take(n as usize) {
        if (0.0..=1.0).contains(&t) {
            let q = p0.add(pv1.mul(2.0 * t)).add(pv2.mul(t * t));
            let diff = q.sub(pt);
            let dsq = diff.dot(diff);
            if dsq < best_sq {
                best_sq = dsq;
            }
        }
    }
    best_sq.sqrt()
}

fn distance_cubic(pt: Vec2, p0: Vec2, p1: Vec2, p2: Vec2, p3: Vec2) -> f32 {
    let mut best = distance_linear(pt, p0, p3);
    for &seed in CUBIC_SEEDS {
        let mut t = seed;
        for _ in 0..NEWTON_ITER {
            let bt = bezier_cubic(t, p0, p1, p2, p3);
            let d1 = bezier_cubic_deriv(t, p0, p1, p2, p3);
            let d2 = bezier_cubic_deriv2(t, p0, p1, p2, p3);
            let qe = bt.sub(pt);
            let denom = d1.dot(d1) + qe.dot(d2);
            if denom.abs() < DEGENERATE_EPS {
                break;
            }
            t -= qe.dot(d1) / denom;
            if t <= 0.0 || t >= 1.0 {
                break;
            }
            let dist = pt.sub(bezier_cubic(t, p0, p1, p2, p3)).length();
            if dist < best {
                best = dist;
            }
        }
    }
    best
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
    winding_subdivided(pt, p0, p2, |t| bezier_quadratic(t, p0, p1, p2), 32)
}

fn winding_cubic(pt: Vec2, p0: Vec2, p1: Vec2, p2: Vec2, p3: Vec2) -> i32 {
    winding_subdivided(pt, p0, p3, |t| bezier_cubic(t, p0, p1, p2, p3), 48)
}

fn rust_port_sdf_byte(pt: Vec2, edges: &[EdgeSegment], sdf_range: f32) -> u8 {
    let mut min_dist = f32::INFINITY;
    let mut winding = 0_i32;
    for e in edges {
        let p0 = Vec2::new(e.points[0], e.points[1]);
        let p1 = Vec2::new(e.points[2], e.points[3]);
        let p2 = Vec2::new(e.points[4], e.points[5]);
        let p3 = Vec2::new(e.points[6], e.points[7]);
        let kind = e.kind & 0b11;
        let (d, w) = if kind == EDGE_KIND_LINEAR {
            (distance_linear(pt, p0, p1), winding_linear(pt, p0, p1))
        } else if kind == EDGE_KIND_QUADRATIC {
            (
                distance_quadratic(pt, p0, p1, p2),
                winding_quadratic(pt, p0, p1, p2),
            )
        } else if kind == EDGE_KIND_CUBIC {
            (
                distance_cubic(pt, p0, p1, p2, p3),
                winding_cubic(pt, p0, p1, p2, p3),
            )
        } else {
            (f32::INFINITY, 0)
        };
        if d < min_dist {
            min_dist = d;
        }
        winding += w;
    }
    let sign = if winding != 0 { 1.0 } else { -1.0 };
    let signed = min_dist * sign;
    let normalized = (signed / sdf_range + 0.5).clamp(0.0, 1.0);
    (normalized * 255.0).to_u8()
}

fn rust_port_bitmap(edges: &[EdgeSegment], width: u32, height: u32, sdf_range: f32) -> Vec<u8> {
    let mut out = Vec::with_capacity((width * height).to_usize());
    for y in 0..height {
        for x in 0..width {
            let pt = Vec2::new(x.to_f32() + 0.5, y.to_f32() + 0.5);
            out.push(rust_port_sdf_byte(pt, edges, sdf_range));
        }
    }
    out
}

fn cpu_sdf_bitmap(font_data: &[u8], ch: char) -> (Vec<u8>, u32, u32) {
    let r = SdfRasterizer::new(CANONICAL_SIZE, SDF_RANGE, PADDING);
    let bitmap = r
        .rasterize(font_data, glyph_index(font_data, ch))
        .unwrap_or_else(|| panic!("CPU SDF returned None for '{ch}'"));
    match bitmap {
        RasterizedBitmap::Sdf(b) => (b.data, b.width, b.height),
        RasterizedBitmap::Msdf(_) => panic!("expected SDF variant"),
    }
}

fn compare_bitmaps(
    label: &str,
    cpu: &[u8],
    gpu_port: &[u8],
    width: u32,
    height: u32,
) -> (i32, f64) {
    assert_eq!(
        cpu.len(),
        gpu_port.len(),
        "{label}: bitmap len mismatch (cpu={}, gpu_port={})",
        cpu.len(),
        gpu_port.len()
    );
    let total = (width * height).to_usize();
    let mut max_diff = 0_i32;
    let mut bad = 0_usize;
    for i in 0..total {
        let diff = i32::from(cpu[i]) - i32::from(gpu_port[i]);
        let abs = diff.abs();
        if abs > max_diff {
            max_diff = abs;
        }
        if abs > TOLERANCE {
            bad += 1;
        }
    }
    let bad_fraction = f64::from(bad.to_f32()) / f64::from(total.to_f32());
    // Dump a diff PNG so failures are debuggable: red where GPU port
    // > CPU, blue where GPU port < CPU, gray otherwise.
    if bad_fraction > 0.0 {
        let mut diff_rgb = Vec::with_capacity(cpu.len() * 3);
        for i in 0..total {
            let c = i32::from(cpu[i]);
            let g = i32::from(gpu_port[i]);
            let d = g - c;
            if d.abs() > TOLERANCE {
                if d > 0 {
                    diff_rgb.extend_from_slice(&[255_u8, 0, 0]);
                } else {
                    diff_rgb.extend_from_slice(&[0, 0, 255]);
                }
            } else {
                diff_rgb.extend_from_slice(&[cpu[i], cpu[i], cpu[i]]);
            }
        }
        let path = std::env::temp_dir().join(format!("bevy_diegetic_gpu_parity_diff_{label}.png"));
        let safe_path: String = path.to_string_lossy().replace(' ', "_").replace('\'', "");
        if let Some(img) = image::RgbImage::from_raw(width, height, diff_rgb) {
            let _ = img.save(&safe_path);
            eprintln!("[parity diff] {label} → {safe_path}");
        }
    }
    (max_diff, bad_fraction)
}

fn run_parity_case(font_data: &[u8], ch: char, font_label: &str) {
    let idx = glyph_index(font_data, ch);
    let Some(body) = edges::build_edge_buffer(font_data, idx, CANONICAL_SIZE, SDF_RANGE, PADDING)
    else {
        panic!("build_edge_buffer returned None for '{ch}'");
    };
    let (cpu, cpu_w, cpu_h) = cpu_sdf_bitmap(font_data, ch);
    assert_eq!(
        (body.bitmap_size.x, body.bitmap_size.y),
        (cpu_w, cpu_h),
        "{font_label} '{ch}': GPU vs CPU bitmap dimensions disagree"
    );
    let _ = (DistanceField::Sdf,); // documentation: parity is SDF-only in Phase 1
    let gpu_port = rust_port_bitmap(
        &body.edges,
        body.bitmap_size.x,
        body.bitmap_size.y,
        SDF_RANGE.to_f32(),
    );
    let (max_diff, bad_fraction) = compare_bitmaps(
        &format!("{font_label} '{ch}'"),
        &cpu,
        &gpu_port,
        cpu_w,
        cpu_h,
    );
    assert!(
        bad_fraction <= MAX_BAD_FRACTION,
        "{font_label} '{ch}': {:.2}% of pixels exceed tolerance ±{TOLERANCE} (max diff {max_diff})",
        bad_fraction * 100.0
    );
}

#[test]
fn parity_jbm_a() { run_parity_case(JETBRAINS_MONO, 'A', "JetBrains Mono"); }

#[test]
fn parity_jbm_w() { run_parity_case(JETBRAINS_MONO, 'W', "JetBrains Mono"); }

#[test]
fn parity_jbm_o() { run_parity_case(JETBRAINS_MONO, 'O', "JetBrains Mono"); }

#[test]
fn parity_ebg_v() { run_parity_case(EB_GARAMOND, 'V', "EB Garamond"); }

#[test]
fn parity_ebg_a() { run_parity_case(EB_GARAMOND, 'A', "EB Garamond"); }
