#![allow(clippy::cast_precision_loss)]
#![allow(clippy::missing_docs_in_private_items)]
#![allow(missing_docs)]
#![allow(clippy::unwrap_used)]

//! Benchmark for per-frame text rendering costs: parley shaping, quad
//! construction from cached shapes, and mesh building.
//!
//! Run with `cargo bench --bench text_render_perf`.

use bevy_diegetic::BoundingBox;
use bevy_diegetic::FontRegistry;
use bevy_diegetic::GlyphQuadData;
use bevy_diegetic::MsdfAtlas;
use bevy_diegetic::ShapedTextCache;
use bevy_diegetic::TextConfig;
use bevy_diegetic::TextShapingContext;
use bevy_diegetic::build_glyph_mesh;
use bevy_diegetic::shape_text_to_quads;
use criterion::Criterion;
use criterion::criterion_group;
use criterion::criterion_main;

const FONT_DATA: &[u8] = include_bytes!("../assets/fonts/JetBrainsMono-Regular.ttf");
const FONT_SIZE: f32 = 7.0;

/// Words used as row values — same as `text_stress` example.
const WORDS: &[&str] = &[
    "bevy",
    "diegetic",
    "layout",
    "engine",
    "text",
    "rendering",
    "msdf",
    "atlas",
    "glyph",
    "quad",
    "mesh",
    "shader",
    "pipeline",
    "parley",
    "shaping",
    "font",
    "registry",
    "plugin",
    "system",
    "resource",
    "component",
    "query",
    "transform",
    "camera",
    "projection",
    "orthographic",
    "perspective",
    "viewport",
    "world",
    "entity",
];

struct TestContext {
    registry:   FontRegistry,
    atlas:      MsdfAtlas,
    shaping_cx: TextShapingContext,
    cache:      ShapedTextCache,
}

impl TestContext {
    fn new() -> Self {
        let registry = FontRegistry::new();
        let mut atlas = MsdfAtlas::new();
        let ascii: String = (33_u8..=126).map(|c| c as char).collect();
        atlas.prepopulate(0, FONT_DATA, &ascii);
        Self {
            registry,
            atlas,
            shaping_cx: TextShapingContext::default(),
            cache: ShapedTextCache::default(),
        }
    }
}

fn generate_rows(count: usize) -> Vec<(String, &'static str)> {
    (0..count)
        .map(|i| (format!("item {i}:"), WORDS[i % WORDS.len()]))
        .collect()
}

// ── Benchmarks ───────────────────────────────────────────────────────────────

fn bench_shape_text(c: &mut Criterion) {
    let mut group = c.benchmark_group("shape_text");

    for count in [1, 10, 50, 100] {
        let rows = generate_rows(count);

        // Cold cache — every string is a miss.
        group.bench_function(format!("cold_{count}_strings"), |b| {
            let mut cx = TestContext::new();
            b.iter(|| {
                cx.cache = ShapedTextCache::default(); // clear cache each iteration
                for (label, value) in &rows {
                    let bounds = BoundingBox {
                        x:      0.0,
                        y:      0.0,
                        width:  100.0,
                        height: 10.0,
                    };
                    let config = TextConfig::new(FONT_SIZE);
                    shape_text_to_quads(
                        label,
                        &config,
                        &bounds,
                        &cx.registry,
                        &mut cx.atlas,
                        &cx.shaping_cx,
                        &mut cx.cache,
                        0.01,
                        0.01,
                        0.5,
                        0.5,
                    );
                    shape_text_to_quads(
                        value,
                        &config,
                        &bounds,
                        &cx.registry,
                        &mut cx.atlas,
                        &cx.shaping_cx,
                        &mut cx.cache,
                        0.01,
                        0.01,
                        0.5,
                        0.5,
                    );
                }
            });
        });

        // Warm cache — every string is a hit.
        group.bench_function(format!("warm_{count}_strings"), |b| {
            let mut cx = TestContext::new();
            // Warm the cache.
            for (label, value) in &rows {
                let bounds = BoundingBox {
                    x:      0.0,
                    y:      0.0,
                    width:  100.0,
                    height: 10.0,
                };
                let config = TextConfig::new(FONT_SIZE);
                shape_text_to_quads(
                    label,
                    &config,
                    &bounds,
                    &cx.registry,
                    &mut cx.atlas,
                    &cx.shaping_cx,
                    &mut cx.cache,
                    0.01,
                    0.01,
                    0.5,
                    0.5,
                );
                shape_text_to_quads(
                    value,
                    &config,
                    &bounds,
                    &cx.registry,
                    &mut cx.atlas,
                    &cx.shaping_cx,
                    &mut cx.cache,
                    0.01,
                    0.01,
                    0.5,
                    0.5,
                );
            }
            b.iter(|| {
                for (label, value) in &rows {
                    let bounds = BoundingBox {
                        x:      0.0,
                        y:      0.0,
                        width:  100.0,
                        height: 10.0,
                    };
                    let config = TextConfig::new(FONT_SIZE);
                    shape_text_to_quads(
                        label,
                        &config,
                        &bounds,
                        &cx.registry,
                        &mut cx.atlas,
                        &cx.shaping_cx,
                        &mut cx.cache,
                        0.01,
                        0.01,
                        0.5,
                        0.5,
                    );
                    shape_text_to_quads(
                        value,
                        &config,
                        &bounds,
                        &cx.registry,
                        &mut cx.atlas,
                        &cx.shaping_cx,
                        &mut cx.cache,
                        0.01,
                        0.01,
                        0.5,
                        0.5,
                    );
                }
            });
        });
    }

    group.finish();
}

fn bench_mesh_build(c: &mut Criterion) {
    let mut group = c.benchmark_group("mesh_build");

    for glyph_count in [100, 500, 1000, 3000] {
        // Generate dummy quads.
        let quads: Vec<GlyphQuadData> = (0..glyph_count)
            .map(|i| {
                let x = (i % 50) as f32 * 0.02;
                let y = (i / 50) as f32 * 0.02;
                GlyphQuadData {
                    position: [x, y, 0.001],
                    size:     [0.01, 0.015],
                    uv_rect:  [0.0, 0.0, 0.03, 0.03],
                    color:    [1.0, 1.0, 1.0, 1.0],
                }
            })
            .collect();

        group.bench_function(format!("{glyph_count}_glyphs"), |b| {
            b.iter(|| build_glyph_mesh(&quads));
        });
    }

    group.finish();
}

criterion_group!(benches, bench_shape_text, bench_mesh_build);
criterion_main!(benches);
