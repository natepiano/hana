//! exposes the VisualizationControl plugin for use in bevy based visualizations
mod control;

/// The `VisualizationPlugin` is a Bevy plugin for hana's remote control of your visualization.
///
/// # Example
///
/// ```rust
/// # use hana_visualization::VisualizationControl;
/// # use bevy::prelude::*;
///
///  App::new()
///      .add_plugins(VisualizationControl)
///      // other app setup code
///      .run();
///
/// ```
pub use control::VisualizationControl;
