//! Parley-backed text measurement.

use std::sync::Arc;
use std::sync::Mutex;
use std::sync::PoisonError;

use parley::FontContext;
use parley::Layout;
use parley::LayoutContext;
use parley::style::FontFamily;
use parley::style::FontStyle;
use parley::style::FontWeight;
use parley::style::LineHeight;
use parley::style::StyleProperty;

use crate::FontSlant;
use crate::layout::MeasureTextFn;
use crate::layout::TextDimensions;
use crate::layout::TextMeasure;

/// Creates a [`MeasureTextFn`] backed by parley's layout engine.
///
/// Uses the shared [`FontContext`] from the registry and creates its own
/// [`LayoutContext`] and [`Layout`] buffer for text measurement.
///
/// The returned closure is `Send + Sync` via `Arc<Mutex<>>` wrappers
/// on the mutable parley contexts.
#[must_use]
pub fn create_parley_measurer(
    font_cx: Arc<Mutex<FontContext>>,
    families: Vec<String>,
) -> MeasureTextFn {
    let layout_cx: Mutex<LayoutContext<()>> = Mutex::new(LayoutContext::default());
    let layout_buf: Mutex<Layout<()>> = Mutex::new(Layout::new());

    Arc::new(move |text: &str, measure: &TextMeasure| {
        let family_name = families
            .get(measure.font_id as usize)
            .map_or("JetBrains Mono", String::as_str);

        let mut font_cx = font_cx.lock().unwrap_or_else(PoisonError::into_inner);
        let mut layout_cx = layout_cx.lock().unwrap_or_else(PoisonError::into_inner);
        let mut layout = layout_buf.lock().unwrap_or_else(PoisonError::into_inner);

        let mut builder = layout_cx.ranged_builder(&mut font_cx, text, 1.0, true);
        builder.push_default(StyleProperty::FontSize(measure.size));
        builder.push_default(StyleProperty::FontFamily(FontFamily::named(family_name)));
        builder.push_default(StyleProperty::FontWeight(FontWeight::new(measure.weight.0)));

        let font_style = match measure.slant {
            FontSlant::Normal => FontStyle::Normal,
            FontSlant::Italic => FontStyle::Italic,
            FontSlant::Oblique => FontStyle::Oblique(None),
        };
        builder.push_default(StyleProperty::FontStyle(font_style));

        if measure.letter_spacing != 0.0 {
            builder.push_default(StyleProperty::LetterSpacing(measure.letter_spacing));
        }
        if measure.word_spacing != 0.0 {
            builder.push_default(StyleProperty::WordSpacing(measure.word_spacing));
        }

        if measure.line_height > 0.0 {
            builder.push_default(StyleProperty::LineHeight(LineHeight::Absolute(
                measure.line_height,
            )));
        }

        // Push OpenType feature overrides (liga, calt, dlig, kern).
        let font_features = measure.font_features;
        if !font_features.is_default() {
            let parley_features: Vec<parley::style::FontFeature> = font_features
                .to_parley_settings()
                .into_iter()
                .map(|(tag, value)| parley::FontFeature {
                    tag: parley::setting::Tag::from_bytes(tag),
                    value,
                })
                .collect();
            builder.push_default(StyleProperty::FontFeatures(
                parley::style::FontFeatures::List(std::borrow::Cow::Owned(parley_features)),
            ));
        }

        builder.build_into(&mut layout, text);
        layout.break_all_lines(None);

        // Drop mutable guards before reading layout results.
        drop(font_cx);
        drop(layout_cx);

        let line_height = layout
            .lines()
            .next()
            .map_or(measure.size, |l| l.metrics().line_height);

        TextDimensions {
            width: layout.full_width(),
            height: layout.height(),
            line_height,
        }
    })
}
