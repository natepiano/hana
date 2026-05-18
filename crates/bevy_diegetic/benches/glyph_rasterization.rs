//! Benchmarks for parallel glyph rasterization.
//!
//! Measures the public [`GlyphAtlas`] async path: queue N glyphs, then
//! busy-poll until every result has drained back through the channel.
//!
//! Run with `cargo bench -p bevy_diegetic --bench glyph_rasterization`.
//! Save a baseline before applying a fix and compare afterward:
//!
//! ```text
//! cargo bench -p bevy_diegetic --bench glyph_rasterization -- --save-baseline before
//! # ... apply a fix ...
//! cargo bench -p bevy_diegetic --bench glyph_rasterization -- --baseline before
//! ```
//!
//! ## Groups
//!
//! - `warmup_burst` — full ASCII warm-up across font × px-size × mode.
//!   The headline number a user feels when swapping `RasterQuality`.
//! - `thread_scaling` — same workload at 1 / 2 / 4 / 6 / 8 / 12 worker
//!   threads. Reveals the empirical ceiling for adding more threads.
//! - `single_glyph` — one glyph at one worker, isolated per-glyph wall
//!   time independent of dispatch and load-balancing.
//! - `face_parse` — `ttf-parser` [`Face::parse`] cost. Per-glyph today,
//!   bounds the win from caching it.
//! - `image_alloc` — [`Rgb32FImage::new`] cost at the bbox sizes the
//!   MSDF generator produces. Bounds the win from buffer reuse.

use std::hint::black_box;
use std::sync::Arc;
use std::thread;

use bevy::tasks::TaskPool;
use bevy::tasks::TaskPoolBuilder;
use bevy_diegetic::DistanceField;
use bevy_diegetic::GlyphAtlas;
use bevy_diegetic::GlyphKey;
use criterion::Criterion;
use criterion::Throughput;
use criterion::criterion_group;
use criterion::criterion_main;
use image::Rgb32FImage;
use ttf_parser::Face;

const ATLAS_PAGE_SIZE: u32 = 1024;
const ASCII_PRINTABLE: &str =
    "!\"#$%&'()*+,-./0123456789:;<=>?@ABCDEFGHIJKLMNOPQRSTUVWXYZ[\\]^_`abcdefghijklmnopqrstuvwxyz{|}~";

const JETBRAINS_MONO_DATA: &[u8] = include_bytes!("../assets/fonts/JetBrainsMono-Regular.ttf");
const EB_GARAMOND_DATA: &[u8] = include_bytes!("../assets/fonts/EBGaramond-Regular.ttf");

const FONT_ID: u16 = 0;

fn shared_pool(threads: usize) -> Arc<TaskPool> {
    Arc::new(
        TaskPoolBuilder::new()
            .num_threads(threads)
            .thread_name("bench glyph raster".to_string())
            .build(),
    )
}

fn queue_all(atlas: &mut GlyphAtlas, font_data: &[u8], text: &str) {
    let Ok(face) = Face::parse(font_data, 0) else {
        return;
    };
    for ch in text.chars() {
        if let Some(gid) = face.glyph_index(ch) {
            atlas.get_or_insert(
                GlyphKey {
                    font_id:     FONT_ID,
                    glyph_index: gid.0,
                },
                font_data,
            );
        }
    }
}

fn drain(atlas: &mut GlyphAtlas) {
    while atlas.in_flight_count() > 0 {
        atlas.poll_async_glyphs();
        thread::yield_now();
    }
}

fn count_glyphs(font_data: &[u8], text: &str) -> u64 {
    Face::parse(font_data, 0).map_or(0, |face| {
        text.chars()
            .filter(|c| face.glyph_index(*c).is_some())
            .count() as u64
    })
}

struct WarmupCase {
    name:           &'static str,
    font_data:      &'static [u8],
    canonical_size: u32,
    distance_field: DistanceField,
}

const WARMUP_CASES: &[WarmupCase] = &[
    WarmupCase {
        name:           "jbm_ascii_128_msdf",
        font_data:      JETBRAINS_MONO_DATA,
        canonical_size: 128,
        distance_field: DistanceField::Msdf,
    },
    WarmupCase {
        name:           "jbm_ascii_256_msdf",
        font_data:      JETBRAINS_MONO_DATA,
        canonical_size: 256,
        distance_field: DistanceField::Msdf,
    },
    WarmupCase {
        name:           "jbm_ascii_128_sdf",
        font_data:      JETBRAINS_MONO_DATA,
        canonical_size: 128,
        distance_field: DistanceField::Sdf,
    },
    WarmupCase {
        name:           "ebg_ascii_128_msdf",
        font_data:      EB_GARAMOND_DATA,
        canonical_size: 128,
        distance_field: DistanceField::Msdf,
    },
    WarmupCase {
        name:           "ebg_ascii_256_msdf",
        font_data:      EB_GARAMOND_DATA,
        canonical_size: 256,
        distance_field: DistanceField::Msdf,
    },
];

// Keep in sync with `DEFAULT_GLYPH_WORKER_THREADS` in
// `src/text/constants.rs` so `warmup_burst` reflects production wall-time.
const WARMUP_THREADS: usize = 8;

fn bench_warmup_burst(c: &mut Criterion) {
    let pool = shared_pool(WARMUP_THREADS);
    let mut group = c.benchmark_group("warmup_burst");
    group.sample_size(20);
    for case in WARMUP_CASES {
        group.throughput(Throughput::Elements(count_glyphs(
            case.font_data,
            ASCII_PRINTABLE,
        )));
        group.bench_function(case.name, |b| {
            b.iter_with_setup(
                || {
                    GlyphAtlas::with_config(
                        ATLAS_PAGE_SIZE,
                        case.canonical_size,
                        WARMUP_THREADS,
                        case.distance_field,
                        Some(Arc::clone(&pool)),
                    )
                },
                |mut atlas| {
                    queue_all(&mut atlas, case.font_data, ASCII_PRINTABLE);
                    drain(&mut atlas);
                    black_box(atlas.glyph_count());
                },
            );
        });
    }
    group.finish();
}

const THREAD_COUNTS: &[usize] = &[1, 2, 4, 6, 8, 12];
const SCALING_CANONICAL: u32 = 128;

fn bench_thread_scaling(c: &mut Criterion) {
    let mut group = c.benchmark_group("thread_scaling");
    group.sample_size(20);
    group.throughput(Throughput::Elements(count_glyphs(
        JETBRAINS_MONO_DATA,
        ASCII_PRINTABLE,
    )));
    for &threads in THREAD_COUNTS {
        let pool = shared_pool(threads);
        group.bench_function(format!("{threads:02}_threads"), |b| {
            b.iter_with_setup(
                || {
                    GlyphAtlas::with_config(
                        ATLAS_PAGE_SIZE,
                        SCALING_CANONICAL,
                        threads,
                        DistanceField::Msdf,
                        Some(Arc::clone(&pool)),
                    )
                },
                |mut atlas| {
                    queue_all(&mut atlas, JETBRAINS_MONO_DATA, ASCII_PRINTABLE);
                    drain(&mut atlas);
                    black_box(atlas.glyph_count());
                },
            );
        });
    }
    group.finish();
}

struct SingleGlyphCase {
    name:           &'static str,
    font_data:      &'static [u8],
    character:      char,
    canonical_size: u32,
}

const SINGLE_GLYPH_CASES: &[SingleGlyphCase] = &[
    SingleGlyphCase {
        name:           "jbm_A_128",
        font_data:      JETBRAINS_MONO_DATA,
        character:      'A',
        canonical_size: 128,
    },
    SingleGlyphCase {
        name:           "jbm_A_256",
        font_data:      JETBRAINS_MONO_DATA,
        character:      'A',
        canonical_size: 256,
    },
    SingleGlyphCase {
        name:           "jbm_W_128",
        font_data:      JETBRAINS_MONO_DATA,
        character:      'W',
        canonical_size: 128,
    },
    SingleGlyphCase {
        name:           "ebg_V_128",
        font_data:      EB_GARAMOND_DATA,
        character:      'V',
        canonical_size: 128,
    },
    SingleGlyphCase {
        name:           "ebg_V_256",
        font_data:      EB_GARAMOND_DATA,
        character:      'V',
        canonical_size: 256,
    },
];

fn bench_single_glyph(c: &mut Criterion) {
    let pool = shared_pool(1);
    let mut group = c.benchmark_group("single_glyph");
    group.sample_size(30);
    for case in SINGLE_GLYPH_CASES {
        let face =
            Face::parse(case.font_data, 0).unwrap_or_else(|e| panic!("parse font: {e}"));
        let glyph_index = face
            .glyph_index(case.character)
            .unwrap_or_else(|| panic!("no glyph for '{}'", case.character))
            .0;
        group.bench_function(case.name, |b| {
            b.iter_with_setup(
                || {
                    GlyphAtlas::with_config(
                        ATLAS_PAGE_SIZE,
                        case.canonical_size,
                        1,
                        DistanceField::Msdf,
                        Some(Arc::clone(&pool)),
                    )
                },
                |mut atlas| {
                    atlas.get_or_insert(
                        GlyphKey {
                            font_id: FONT_ID,
                            glyph_index,
                        },
                        case.font_data,
                    );
                    drain(&mut atlas);
                    black_box(atlas.glyph_count());
                },
            );
        });
    }
    group.finish();
}

fn bench_face_parse(c: &mut Criterion) {
    let mut group = c.benchmark_group("face_parse");
    for (name, data) in [
        ("jetbrains_mono", JETBRAINS_MONO_DATA),
        ("eb_garamond", EB_GARAMOND_DATA),
    ] {
        group.bench_function(name, |b| {
            b.iter(|| black_box(Face::parse(data, 0).expect("parse")));
        });
    }
    group.finish();
}

const IMAGE_SIZES: &[u32] = &[64, 128, 256];

fn bench_image_alloc(c: &mut Criterion) {
    let mut group = c.benchmark_group("image_alloc");
    for &side in IMAGE_SIZES {
        group.bench_function(format!("rgb32f_{side}x{side}"), |b| {
            b.iter(|| black_box(Rgb32FImage::new(side, side)));
        });
    }
    group.finish();
}

criterion_group!(
    benches,
    bench_warmup_burst,
    bench_thread_scaling,
    bench_single_glyph,
    bench_face_parse,
    bench_image_alloc,
);
criterion_main!(benches);
