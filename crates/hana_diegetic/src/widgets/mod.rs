mod button;
mod id;
mod reify;
mod relationship;
mod slider;

use bevy::ecs::schedule::ApplyDeferred;
use bevy::prelude::*;
pub use button::Button;
pub(crate) use id::ComputedWidgetRecord;
pub use id::PanelWidget;
pub use id::PanelWidgetReader;
pub(crate) use id::WidgetKind;
pub(crate) use id::WidgetSpec;
pub(crate) use id::validate_tree;
pub use relationship::PanelWidgets;
pub use relationship::WidgetOf;
pub use slider::Slider;
pub use slider::SliderConfigError;
pub use slider::SliderDirection;
pub use slider::SliderRange;
pub use slider::SliderStep;

use crate::PanelSystems;

/// Named scheduling points for semantic widget work.
#[derive(SystemSet, Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(crate) enum WidgetSystems {
    /// Reifies widget identities from the latest computed panel output.
    Reify,
}

/// Installs headless panel widget identity and reification.
pub(crate) struct WidgetsPlugin;

impl Plugin for WidgetsPlugin {
    fn build(&self, app: &mut App) {
        app.configure_sets(
            Update,
            WidgetSystems::Reify
                .after(PanelSystems::ComputeLayout)
                .before(PanelSystems::ResolvePanelAttachments),
        )
        .add_systems(
            Update,
            (
                reify::reify_widgets.in_set(WidgetSystems::Reify),
                ApplyDeferred
                    .after(WidgetSystems::Reify)
                    .before(PanelSystems::ResolvePanelAttachments),
            ),
        );
    }
}
