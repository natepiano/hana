//! Tests for the parley-backed text measurer.
//!
//! Verifies that real font measurement via parley produces reasonable dimensions
//! from the embedded `JetBrains Mono` font.

#![allow(
    clippy::unwrap_used,
    reason = "tests use unwrap for clearer failure messages"
)]

use super::FontRegistry;
use super::create_parley_measurer;
use crate::LayoutTextStyle;
use crate::MeasureTextFn;
use crate::TextMeasure;

fn measurer() -> MeasureTextFn {
    let registry = FontRegistry::new().unwrap();
    create_parley_measurer(registry.font_context(), registry.family_names())
}

fn default_measure(size: f32) -> TextMeasure { LayoutTextStyle::new(size).as_measure() }

// ── Basic measurement ────────────────────────────────────────────────────────

#[test]
fn measures_nonzero_dimensions() {
    let measure = measurer();
    let dims = measure("Hello", &default_measure(16.0));
    assert!(
        dims.width > 0.0,
        "width should be positive, got {}",
        dims.width
    );
    assert!(
        dims.height > 0.0,
        "height should be positive, got {}",
        dims.height
    );
}

#[test]
fn empty_string_is_narrower_than_content() {
    let measure = measurer();
    let m = default_measure(16.0);
    let empty = measure("", &m);
    let content = measure("Hello", &m);
    assert!(
        empty.width < content.width,
        "empty string should be narrower than content: {:.2} vs {:.2}",
        empty.width,
        content.width
    );
}

#[test]
fn longer_text_is_wider() {
    let measure = measurer();
    let m = default_measure(16.0);
    let short = measure("Hi", &m);
    let long = measure("Hello, world!", &m);
    assert!(
        long.width > short.width,
        "longer text should be wider: {:.2} vs {:.2}",
        long.width,
        short.width
    );
}

// ── Font size scaling ────────────────────────────────────────────────────────

#[test]
fn larger_font_produces_wider_text() {
    let measure = measurer();
    let small = measure("Hello", &default_measure(10.0));
    let large = measure("Hello", &default_measure(20.0));
    assert!(
        large.width > small.width,
        "20pt should be wider than 10pt: {:.2} vs {:.2}",
        large.width,
        small.width
    );
}

#[test]
fn larger_font_produces_taller_text() {
    let measure = measurer();
    let small = measure("Hello", &default_measure(10.0));
    let large = measure("Hello", &default_measure(20.0));
    assert!(
        large.height > small.height,
        "20pt should be taller than 10pt: {:.2} vs {:.2}",
        large.height,
        small.height
    );
}

#[test]
fn width_scales_roughly_with_font_size() {
    let measure = measurer();
    let small = measure("Hello", &default_measure(10.0));
    let large = measure("Hello", &default_measure(20.0));
    let ratio = large.width / small.width;
    assert!(
        (1.5..2.5).contains(&ratio),
        "2x font size should roughly double width, got ratio {ratio:.2}"
    );
}

// ── Monospace property ───────────────────────────────────────────────────────

#[test]
fn monospace_equal_length_strings_have_similar_width() {
    let measure = measurer();
    let m = default_measure(16.0);
    let a = measure("iiiii", &m);
    let b = measure("MMMMM", &m);
    // JetBrains Mono is monospace — same character count should be same width.
    let diff = (a.width - b.width).abs();
    assert!(
        diff < 1.0,
        "monospace font: 'iiiii' and 'MMMMM' should have similar width, diff={diff:.2}"
    );
}

// ── Weight affects measurement ───────────────────────────────────────────────

#[test]
fn bold_text_is_at_least_as_wide() {
    let measure = measurer();
    let normal = measure("Hello", &default_measure(16.0));
    let bold_measure = LayoutTextStyle::new(16.0).bold().as_measure();
    let bold = measure("Hello", &bold_measure);
    // Bold glyphs are typically slightly wider. With a single weight font file
    // (Regular only), parley may synthesize or return the same width. Either
    // way, bold should never be narrower.
    assert!(
        bold.width >= normal.width - 0.5,
        "bold should not be narrower: bold={:.2} normal={:.2}",
        bold.width,
        normal.width
    );
}

// ── Multiline ────────────────────────────────────────────────────────────────

#[test]
fn newline_increases_height() {
    let measure = measurer();
    let m = default_measure(16.0);
    let one_line = measure("Hello", &m);
    let two_lines = measure("Hello\nWorld", &m);
    assert!(
        two_lines.height > one_line.height,
        "two lines should be taller: {:.2} vs {:.2}",
        two_lines.height,
        one_line.height
    );
}

// ── FontId fallback ──────────────────────────────────────────────────────────

#[test]
fn unknown_font_id_still_measures() {
    let measure = measurer();
    let mut m = default_measure(16.0);
    m.font_id = 999; // No such font registered.
    let dims = measure("Hello", &m);
    // Should fall back to default font, not panic or return zero.
    assert!(
        dims.width > 0.0,
        "unknown font_id should still measure, got width {}",
        dims.width
    );
}
