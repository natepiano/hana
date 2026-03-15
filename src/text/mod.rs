//! Font loading and text measurement.
//!
//! [`FontRegistry`] manages font loading via parley's `FontContext`.
//! The registry embeds `JetBrains Mono` as the default font and provides
//! a [`MeasureTextFn`](crate::MeasureTextFn) backed by parley's layout engine.

mod font_registry;
mod measurer;

pub use font_registry::FontId;
pub use font_registry::FontRegistry;
pub use measurer::create_parley_measurer;
