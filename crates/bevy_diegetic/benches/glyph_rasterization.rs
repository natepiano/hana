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
//! - `warmup_burst` — full ASCII warm-up across font × px-size × mode. The headline number a user
//!   feels when swapping `RasterQuality`.
//! - `thread_scaling` — same workload at 1 / 2 / 4 / 6 / 8 / 12 worker threads. Reveals the
//!   empirical ceiling for adding more threads.
//! - `single_glyph` — one glyph at one worker, isolated per-glyph wall time independent of dispatch
//!   and load-balancing.
//! - `face_parse` — `ttf-parser` [`Face::parse`] cost. Per-glyph today, bounds the win from caching
//!   it.
//! - `image_alloc` — [`Rgb32FImage::new`] cost at the bbox sizes the MSDF generator produces.
//!   Bounds the win from buffer reuse.

#![allow(
    clippy::panic,
    clippy::expect_used,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "benchmark setup panics on missing test fixtures; bitmap-size math mirrors the production path's bounded f64→u32 cast"
)]

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
const ASCII_PRINTABLE: &str = "!\"#$%&'()*+,-./0123456789:;<=>?@ABCDEFGHIJKLMNOPQRSTUVWXYZ[\\]^_`abcdefghijklmnopqrstuvwxyz{|}~";

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
        let face = Face::parse(case.font_data, 0).unwrap_or_else(|e| panic!("parse font: {e}"));
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

/// CPU-side cost of the GPU rasterization path: bitmap-size
/// computation, page-region allocation, edge-buffer build on the
/// worker pool. **Does not include actual GPU compute time**, which
/// requires a live wgpu device and render-schedule loop — Phase 1.5
/// follow-up. The number this group reports is the main-thread cost
/// the user actually feels when GPU mode is dispatching glyphs, which
/// is the relevant comparison for "does GPU avoid the rasterization
/// hitch?". A small number here vs the CPU path means the GPU path
/// frees the main thread, even if total GPU latency is higher.
fn bench_gpu_main_thread(c: &mut Criterion) {
    use std::sync::mpsc;

    use bevy::math::UVec2;
    use bevy_diegetic::GpuAtlasRegion;

    // The atlas's allocator only needs page size + canonical size for
    // GPU-mode operation; backend is set so any future internal
    // branching follows the right path.
    let pool = shared_pool(WARMUP_THREADS);
    let mut group = c.benchmark_group("warmup_burst_gpu_main_thread");
    group.sample_size(20);
    for case in WARMUP_CASES {
        if !matches!(case.distance_field, DistanceField::Sdf) {
            continue; // Phase 1 GPU is SDF-only
        }
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
                    let face = Face::parse(case.font_data, 0).expect("parse font");
                    let (tx, rx) = mpsc::channel::<GpuAtlasRegion>();
                    let mut queued = 0_usize;
                    for ch in ASCII_PRINTABLE.chars() {
                        let Some(gid) = face.glyph_index(ch) else {
                            continue;
                        };
                        // Synchronous bitmap-size + allocate (the
                        // path enqueue_gpu_glyph takes before
                        // spawning the worker task).
                        let Some(bitmap_size) =
                            bench_glyph_bitmap_size(case.font_data, gid.0, case.canonical_size)
                        else {
                            continue;
                        };
                        let Some(region) = atlas.allocate_gpu_region(bitmap_size) else {
                            continue;
                        };
                        let tx = tx.clone();
                        let font_data = case.font_data.to_vec();
                        let canonical_size = case.canonical_size;
                        atlas
                            .worker_pool()
                            .spawn(async move {
                                let _ = bench_build_edge_buffer(&font_data, gid.0, canonical_size);
                                let _ = tx.send(region);
                            })
                            .detach();
                        queued += 1;
                    }
                    drop(tx);
                    let mut completed = 0_usize;
                    while completed < queued {
                        match rx.recv() {
                            Ok(_) => completed += 1,
                            Err(_) => break,
                        }
                    }
                    black_box(atlas.glyph_count());
                    let _: UVec2 = UVec2::ZERO; // type used in setup, silence unused
                },
            );
        });
    }
    group.finish();
}

/// Mirror of `gpu_rasterizer::edges::glyph_bitmap_size`. The bench can
/// not depend on private crate items, so this is a verbatim copy of
/// the math (kept in sync via the shared `compute_bitmap_size` formula
/// used by both CPU rasterizers).
fn bench_glyph_bitmap_size(
    font_data: &[u8],
    glyph_index: u16,
    canonical_size: u32,
) -> Option<bevy::math::UVec2> {
    let face = Face::parse(font_data, 0).ok()?;
    let bbox = face.glyph_bounding_box(ttf_parser::GlyphId(glyph_index))?;
    let units_per_em = f64::from(face.units_per_em());
    let scale = f64::from(canonical_size) / units_per_em;
    let total_pad = 2.0_f64 + 4.0_f64;
    let glyph_width = f64::from(bbox.x_max - bbox.x_min) * scale;
    let glyph_height = f64::from(bbox.y_max - bbox.y_min) * scale;
    let width = total_pad.mul_add(2.0, glyph_width).ceil() as u32;
    let height = total_pad.mul_add(2.0, glyph_height).ceil() as u32;
    if width == 0 || height == 0 {
        return None;
    }
    Some(bevy::math::UVec2::new(width, height))
}

/// Mirror of `gpu_rasterizer::edges::build_edge_buffer` without the
/// segment packing — the bench measures the cost of loading the
/// outline and walking contours, which is what the spawned worker
/// task in `enqueue_gpu_glyph` actually does.
fn bench_build_edge_buffer(font_data: &[u8], glyph_index: u16, _canonical_size: u32) -> bool {
    let Ok(face) = Face::parse(font_data, 0) else {
        return false;
    };
    let glyph_id = ttf_parser::GlyphId(glyph_index);
    fdsm_ttf_parser::load_shape_from_face(&face, glyph_id).is_some() // allow-banned: upstream fdsm API name
}

criterion_group!(
    benches,
    bench_warmup_burst,
    bench_thread_scaling,
    bench_single_glyph,
    bench_face_parse,
    bench_image_alloc,
    bench_gpu_main_thread,
);
criterion_main!(benches);
