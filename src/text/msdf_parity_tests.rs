//! MSDF parity tests: `fdsm` (pure Rust) vs `msdfgen` (C++ reference).
//!
//! Both engines use different framing (fdsm: tight bounding box, msdfgen:
//! autoframe to a square), so raw pixel comparison doesn't work. Instead,
//! we "render" each MSDF by taking `median(R, G, B)` and thresholding to
//! get a binary inside/outside mask, then compare the fraction of "inside"
//! pixels. This validates that both produce the same glyph shape.
//!
//! Also tests UTF-8 glyph rasterization across multiple scripts using
//! `Noto Sans Regular`.

use std::fmt::Write;

use msdfgen::Bitmap;
use msdfgen::FontExt;
use msdfgen::MsdfGeneratorConfig;
use msdfgen::Range;
use msdfgen::Rgb;
use ttf_parser_018 as ttf018;

use super::msdf_rasterizer::rasterize_glyph;

/// Embedded font data.
const JETBRAINS_MONO: &[u8] = include_bytes!("../../assets/fonts/JetBrainsMono-Regular.ttf");
const NOTO_SANS: &[u8] = include_bytes!("../../assets/fonts/NotoSans-Regular.ttf");

/// SDF range in pixels.
const SDF_RANGE: f64 = 4.0;

/// Padding for fdsm.
const PADDING: u32 = 2;

/// Canonical render size for comparison.
const COMPARE_SIZE: u32 = 64;

/// Maximum acceptable difference in fill fraction between engines.
///
/// Set to 15% to accommodate framing differences (fdsm: tight bounding box,
/// msdfgen: square autoframe). Some glyphs with thin features or complex
/// outlines show higher variance in fill fraction due to different pixel
/// coverage at different scales/offsets.
const FILL_FRACTION_TOLERANCE: f64 = 0.15;

// ── Helpers ──────────────────────────────────────────────────────────────────

fn glyph_index_for(font_data: &[u8], ch: char) -> Option<u16> {
    let face = ttf_parser::Face::parse(font_data, 0).ok()?;
    face.glyph_index(ch).map(|id| id.0)
}

fn glyph_index_018_for(font_data: &[u8], ch: char) -> Option<ttf018::GlyphId> {
    let face = ttf018::Face::parse(font_data, 0).ok()?;
    face.glyph_index(ch)
}

/// Compute fill fraction from an RGB MSDF bitmap.
///
/// "Renders" the MSDF: `median(R, G, B) > 128` means inside the glyph.
fn fill_fraction_rgb(data: &[u8], width: u32, height: u32) -> f64 {
    let total = f64::from(width * height);
    if total == 0.0 {
        return 0.0;
    }

    let mut inside = 0_u64;
    for i in 0..(width * height) as usize {
        let red = data[i * 3];
        let green = data[i * 3 + 1];
        let blue = data[i * 3 + 2];
        let median = median_of_three(red, green, blue);
        if median > 128 {
            inside += 1;
        }
    }

    inside as f64 / total
}

fn median_of_three(a: u8, b: u8, c: u8) -> u8 {
    let mut arr = [a, b, c];
    arr.sort_unstable();
    arr[1]
}

fn float_to_byte(v: f32) -> u8 { (v * 255.0).round().clamp(0.0, 255.0) as u8 }

/// Generate MSDF with fdsm. Returns fill fraction.
fn fdsm_fill(font_data: &[u8], ch: char, px_size: u32) -> Option<f64> {
    let idx = glyph_index_for(font_data, ch)?;
    let bitmap = rasterize_glyph(font_data, idx, px_size, SDF_RANGE, PADDING)?;
    Some(fill_fraction_rgb(&bitmap.data, bitmap.width, bitmap.height))
}

/// Generate MSDF with msdfgen. Returns fill fraction.
fn msdfgen_fill(font_data: &[u8], ch: char, px_size: u32) -> Option<f64> {
    let face = ttf018::Face::parse(font_data, 0).ok()?;
    let glyph_id = glyph_index_018_for(font_data, ch)?;
    let mut shape = face.glyph_shape(glyph_id)?;

    let bound = shape.get_bound();
    let framing = bound.autoframe(px_size, px_size, Range::Px(SDF_RANGE), None)?;

    shape.edge_coloring_simple(3.0, 0);
    let config = MsdfGeneratorConfig::default();

    let mut bitmap = Bitmap::<Rgb<f32>>::new(px_size, px_size);
    shape.generate_msdf(&mut bitmap, framing, config);

    let width = bitmap.width();
    let height = bitmap.height();
    let mut pixels = Vec::with_capacity((width * height * 3) as usize);
    for y in 0..height {
        for x in 0..width {
            let px = bitmap.pixel(x, y);
            pixels.push(float_to_byte(px.r));
            pixels.push(float_to_byte(px.g));
            pixels.push(float_to_byte(px.b));
        }
    }

    Some(fill_fraction_rgb(&pixels, width, height))
}

struct ParityResult {
    ch:           char,
    script:       &'static str,
    px_size:      u32,
    fdsm_fill:    Option<f64>,
    msdfgen_fill: Option<f64>,
    diff:         Option<f64>,
    pass:         bool,
}

fn run_parity(font_data: &[u8], ch: char, script: &'static str, px_size: u32) -> ParityResult {
    let fdsm = fdsm_fill(font_data, ch, px_size);
    let msdfgen = msdfgen_fill(font_data, ch, px_size);

    let (diff, pass) = match (fdsm, msdfgen) {
        (Some(f), Some(m)) => {
            let d = (f - m).abs();
            (
                Some(d),
                d <= FILL_FRACTION_TOLERANCE && f > 0.01 && m > 0.01,
            )
        },
        (None, None) => (None, true), // Both agree: no outline
        _ => (None, false),           // Disagree on whether glyph exists
    };

    ParityResult {
        ch,
        script,
        px_size,
        fdsm_fill: fdsm,
        msdfgen_fill: msdfgen,
        diff,
        pass,
    }
}

fn format_report(results: &[ParityResult]) -> String {
    let mut report = String::new();
    writeln!(report, "MSDF Parity Report: fdsm vs msdfgen (C++)").unwrap();
    writeln!(report, "=========================================").unwrap();
    writeln!(report).unwrap();
    writeln!(
        report,
        "{:<6} {:<12} {:<5} {:<10} {:<10} {:<8} {:<6}",
        "Glyph", "Script", "Size", "fdsm", "msdfgen", "Diff", "Pass"
    )
    .unwrap();
    writeln!(report, "{}", "-".repeat(62)).unwrap();

    for r in results {
        let fdsm_str = r
            .fdsm_fill
            .map_or_else(|| "None".to_string(), |f| format!("{f:.3}"));
        let msdfgen_str = r
            .msdfgen_fill
            .map_or_else(|| "None".to_string(), |f| format!("{f:.3}"));
        let diff_str = r
            .diff
            .map_or_else(|| "N/A".to_string(), |d| format!("{d:.3}"));
        let pass_str = if r.pass { "OK" } else { "FAIL" };

        writeln!(
            report,
            "'{:<4}' {:<12} {:<5} {:<10} {:<10} {:<8} {:<6}",
            r.ch, r.script, r.px_size, fdsm_str, msdfgen_str, diff_str, pass_str
        )
        .unwrap();
    }

    let total = results.len();
    let passed = results.iter().filter(|r| r.pass).count();
    let failed = total - passed;
    writeln!(report).unwrap();
    writeln!(report, "Total: {total}  Passed: {passed}  Failed: {failed}").unwrap();

    report
}

// ── Full parity report ───────────────────────────────────────────────────────

#[test]
fn parity_report_jetbrains_mono() {
    let font = JETBRAINS_MONO;

    let test_cases: Vec<(char, &str, u32)> = vec![
        // Simple ASCII
        ('I', "Latin", COMPARE_SIZE),
        ('O', "Latin", COMPARE_SIZE),
        ('1', "Digit", COMPARE_SIZE),
        ('.', "Punct", COMPARE_SIZE),
        // Complex ASCII
        ('W', "Latin", COMPARE_SIZE),
        ('@', "Symbol", COMPARE_SIZE),
        ('&', "Symbol", COMPARE_SIZE),
        ('#', "Symbol", COMPARE_SIZE),
        // Size variations
        ('A', "Latin", 24),
        ('A', "Latin", 32),
        ('A', "Latin", 48),
        ('A', "Latin", 64),
    ];

    let results: Vec<ParityResult> = test_cases
        .into_iter()
        .map(|(ch, script, size)| run_parity(font, ch, script, size))
        .collect();

    let report = format_report(&results);
    eprintln!("{report}");

    // Write report to temp.
    let path = std::env::temp_dir().join("bevy_diegetic_parity_report_jetbrains.txt");
    std::fs::write(&path, &report).unwrap();
    eprintln!("Report written to: {}", path.display());

    let failures: Vec<_> = results.iter().filter(|r| !r.pass).collect();
    assert!(
        failures.is_empty(),
        "{} parity failures in JetBrains Mono",
        failures.len()
    );
}

#[test]
fn parity_report_noto_sans() {
    let font = NOTO_SANS;

    let test_cases: Vec<(char, &str, u32)> = vec![
        // Latin
        ('A', "Latin", COMPARE_SIZE),
        ('W', "Latin", COMPARE_SIZE),
        ('g', "Latin", COMPARE_SIZE),
        // Accented Latin
        ('é', "Latin-ext", COMPARE_SIZE),
        ('ñ', "Latin-ext", COMPARE_SIZE),
        ('ü', "Latin-ext", COMPARE_SIZE),
        ('ø', "Latin-ext", COMPARE_SIZE),
        // Cyrillic
        ('Д', "Cyrillic", COMPARE_SIZE),
        ('Ж', "Cyrillic", COMPARE_SIZE),
        ('Щ', "Cyrillic", COMPARE_SIZE),
        ('я', "Cyrillic", COMPARE_SIZE),
        // Greek
        ('Ω', "Greek", COMPARE_SIZE),
        ('Σ', "Greek", COMPARE_SIZE),
        ('φ', "Greek", COMPARE_SIZE),
        // Symbols
        ('€', "Symbol", COMPARE_SIZE),
        ('¥', "Symbol", COMPARE_SIZE),
    ];

    let results: Vec<ParityResult> = test_cases
        .into_iter()
        .map(|(ch, script, size)| run_parity(font, ch, script, size))
        .collect();

    let report = format_report(&results);
    eprintln!("{report}");

    let path = std::env::temp_dir().join("bevy_diegetic_parity_report_noto.txt");
    std::fs::write(&path, &report).unwrap();
    eprintln!("Report written to: {}", path.display());

    let failures: Vec<_> = results.iter().filter(|r| !r.pass).collect();
    assert!(
        failures.is_empty(),
        "{} parity failures in Noto Sans",
        failures.len()
    );
}

// ── UTF-8 rasterization tests (fdsm only) ───────────────────────────────────

#[test]
fn utf8_rasterization_noto_sans() {
    // Only scripts included in Noto Sans Regular (Latin subset).
    // Devanagari, Thai, Arabic, Hebrew are in separate Noto font files.
    let test_chars: Vec<(char, &str)> = vec![
        ('A', "Latin"),
        ('é', "Latin-ext"),
        ('ñ', "Latin-ext"),
        ('ü', "Latin-ext"),
        ('Д', "Cyrillic"),
        ('Ж', "Cyrillic"),
        ('Ω', "Greek"),
        ('Σ', "Greek"),
        ('€', "Symbol"),
        ('¥', "Symbol"),
    ];

    for (ch, script) in &test_chars {
        let idx = glyph_index_for(NOTO_SANS, *ch);
        assert!(
            idx.is_some(),
            "Noto Sans should have glyph for '{ch}' ({script})"
        );

        let bitmap = rasterize_glyph(NOTO_SANS, idx.unwrap(), 32, SDF_RANGE, PADDING);
        assert!(bitmap.is_some(), "fdsm should rasterize '{ch}' ({script})");

        let bmp = bitmap.unwrap();
        assert!(bmp.width > 0, "'{ch}' ({script}) width should be > 0");
        assert!(bmp.height > 0, "'{ch}' ({script}) height should be > 0");

        // MSDF should have varied pixel values.
        let min = bmp.data.iter().copied().min().unwrap_or(0);
        let max = bmp.data.iter().copied().max().unwrap_or(0);
        assert!(
            max - min > 30,
            "'{ch}' ({script}) MSDF should have varied pixels, range [{min}, {max}]"
        );
    }
}

#[test]
fn missing_glyphs_return_none_gracefully() {
    // JetBrains Mono shouldn't have CJK or Devanagari glyphs.
    let missing_chars = ['字', '界', 'अ', 'ก'];

    for ch in missing_chars {
        let idx = glyph_index_for(JETBRAINS_MONO, ch);
        if let Some(idx) = idx {
            // Font has the glyph index but it might have no outline.
            let result = rasterize_glyph(JETBRAINS_MONO, idx, 32, SDF_RANGE, PADDING);
            // Either None (no outline) or Some (font has it) — both OK.
            // The important thing is no panic.
            drop(result);
        }
        // No glyph index at all — also fine, no panic.
    }
}
