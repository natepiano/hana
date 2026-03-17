#![allow(clippy::cast_precision_loss)]
#![allow(clippy::cast_possible_truncation)]
#![allow(clippy::cast_sign_loss)]
#![allow(clippy::missing_docs_in_private_items)]
#![allow(missing_docs)]
#![allow(clippy::unwrap_used)]

//! Benchmark comparing fdsm (pure Rust) and msdfgen (C++ FFI) MSDF generation.
//!
//! Run with `cargo bench --bench msdf_perf`.

use bevy_diegetic::MsdfAtlas;
use bevy_diegetic::rasterize_glyph;
use criterion::Criterion;
use criterion::criterion_group;
use criterion::criterion_main;
use msdfgen::Bitmap;
use msdfgen::FontExt;
use msdfgen::MsdfGeneratorConfig;
use msdfgen::Range;
use msdfgen::Rgb;
use ttf_parser_018 as ttf018;

const FONT_DATA: &[u8] = include_bytes!("../assets/fonts/JetBrainsMono-Regular.ttf");
const SDF_RANGE: f64 = 4.0;
const PX_SIZE: u32 = 32;
const PADDING: u32 = 2;

// ── Helpers ──────────────────────────────────────────────────────────────────

fn glyph_index(ch: char) -> u16 {
    let face = ttf_parser::Face::parse(FONT_DATA, 0).unwrap();
    face.glyph_index(ch).unwrap().0
}

fn glyph_index_018(ch: char) -> ttf018::GlyphId {
    let face = ttf018::Face::parse(FONT_DATA, 0).unwrap();
    face.glyph_index(ch).unwrap()
}

fn fdsm_single(ch: char) {
    let idx = glyph_index(ch);
    let _ = rasterize_glyph(FONT_DATA, idx, PX_SIZE, SDF_RANGE, PADDING);
}

fn msdfgen_single(ch: char) {
    let face = ttf018::Face::parse(FONT_DATA, 0).unwrap();
    let glyph_id = glyph_index_018(ch);
    let mut shape = face.glyph_shape(glyph_id).unwrap();

    let bound = shape.get_bound();
    let framing = bound
        .autoframe(PX_SIZE, PX_SIZE, Range::Px(SDF_RANGE), None)
        .unwrap();

    shape.edge_coloring_simple(3.0, 0);
    let config = MsdfGeneratorConfig::default();

    let mut bitmap = Bitmap::<Rgb<f32>>::new(PX_SIZE, PX_SIZE);
    shape.generate_msdf(&mut bitmap, framing, config);
}

// ── Per-glyph benchmarks ─────────────────────────────────────────────────────

fn bench_single_glyph(c: &mut Criterion) {
    let mut group = c.benchmark_group("single_glyph");

    for ch in ['A', 'W', '@'] {
        group.bench_function(format!("fdsm_{ch}"), |b| b.iter(|| fdsm_single(ch)));
        group.bench_function(format!("msdfgen_{ch}"), |b| b.iter(|| msdfgen_single(ch)));
    }

    group.finish();
}

// ── ASCII batch benchmarks ───────────────────────────────────────────────────

fn bench_ascii_batch(c: &mut Criterion) {
    let ascii: Vec<char> = (33_u8..=126).map(|c| c as char).collect();

    let mut group = c.benchmark_group("ascii_batch");

    group.bench_function("fdsm_94_glyphs", |b| {
        b.iter(|| {
            for &ch in &ascii {
                let idx = glyph_index(ch);
                let _ = rasterize_glyph(FONT_DATA, idx, PX_SIZE, SDF_RANGE, PADDING);
            }
        });
    });

    group.bench_function("msdfgen_94_glyphs", |b| {
        b.iter(|| {
            let face = ttf018::Face::parse(FONT_DATA, 0).unwrap();
            let config = MsdfGeneratorConfig::default();
            for &ch in &ascii {
                let glyph_id = face.glyph_index(ch).unwrap();
                let mut shape = face.glyph_shape(glyph_id).unwrap();
                let bound = shape.get_bound();
                if let Some(framing) = bound.autoframe(PX_SIZE, PX_SIZE, Range::Px(SDF_RANGE), None)
                {
                    shape.edge_coloring_simple(3.0, 0);
                    let mut bitmap = Bitmap::<Rgb<f32>>::new(PX_SIZE, PX_SIZE);
                    shape.generate_msdf(&mut bitmap, framing, config);
                }
            }
        });
    });

    group.finish();
}

// ── Atlas prepopulate benchmark ──────────────────────────────────────────────

fn bench_atlas_prepopulate(c: &mut Criterion) {
    let ascii: String = (33_u8..=126).map(|c| c as char).collect();

    c.bench_function("atlas_prepopulate_ascii", |b| {
        b.iter(|| {
            let mut atlas = MsdfAtlas::new();
            atlas.prepopulate(0, FONT_DATA, &ascii);
        });
    });
}

criterion_group!(
    benches,
    bench_single_glyph,
    bench_ascii_batch,
    bench_atlas_prepopulate,
);
criterion_main!(benches);
