use std::sync::Arc;

use bevy_diegetic::DiegeticTextMeasurer;
use bevy_diegetic::MeasureTextFn;
use bevy_diegetic::TextDimensions;
use bevy_diegetic::TextMeasure;
use bevy_kana::ToF32;
use clay_layout::math::Dimensions;

pub(crate) const FONT_SIZE: f32 = 10.0;
pub(crate) const CLAY_FONT_SIZE: u16 = 10;
pub(crate) const CHAR_WIDTH_FACTOR: f32 = 0.6;

#[must_use = "text dimensions are the benchmarked measurement result"]
pub(crate) fn monospace_measure_text(text: &str, measure: &TextMeasure) -> TextDimensions {
    let char_width = measure.size * CHAR_WIDTH_FACTOR;
    let mut max_line_width: f32 = 0.0;
    let mut line_count = 0_u32;
    for line in text.lines() {
        line_count += 1;
        let width = line.chars().count().to_f32() * char_width;
        max_line_width = max_line_width.max(width);
    }
    if line_count == 0 {
        line_count = 1;
    }
    TextDimensions {
        width:       max_line_width,
        height:      measure.size * line_count.to_f32(),
        line_height: measure.size,
    }
}

#[must_use = "the raw layout engine needs a measurement callback"]
pub(crate) fn monospace_measure_text_fn() -> MeasureTextFn { Arc::new(monospace_measure_text) }

#[must_use = "the headless app needs this resource for panel layout"]
pub(crate) fn monospace_measurer() -> DiegeticTextMeasurer {
    DiegeticTextMeasurer {
        measure_fn: monospace_measure_text_fn(),
    }
}

pub(crate) fn clay_monospace_measure(
    text: &str,
    config: &clay_layout::text::TextConfig,
    _: &mut (),
) -> Dimensions {
    let font_size = f32::from(config.font_size);
    let char_width = font_size * CHAR_WIDTH_FACTOR;
    let line_height = if config.line_height == 0 {
        font_size
    } else {
        f32::from(config.line_height)
    };
    let mut max_line_width: f32 = 0.0;
    let mut line_count = 0_u32;
    for line in text.lines() {
        line_count += 1;
        let width = line.chars().count().to_f32() * char_width;
        max_line_width = max_line_width.max(width);
    }
    if line_count == 0 {
        line_count = 1;
    }
    Dimensions {
        width:  max_line_width,
        height: line_height * line_count.to_f32(),
    }
}
