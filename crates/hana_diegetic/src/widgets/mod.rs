mod button;
mod id;
mod interactivity;
mod reify;
mod relationship;
mod slider;

use bevy::ecs::schedule::ApplyDeferred;
use bevy::prelude::*;
pub use button::Button;
pub(crate) use id::ComputedWidgetRecord;
pub use id::PanelWidget;
pub(crate) use id::PanelWidgetIndex;
pub use id::PanelWidgetReader;
pub(crate) use id::WidgetKind;
pub(crate) use id::WidgetSpec;
pub(crate) use id::validate_tree;
pub use interactivity::PanelWidgetWriter;
pub use interactivity::WidgetDisabled;
pub use interactivity::WidgetInteractivity;
pub use relationship::PanelWidgets;
pub use relationship::WidgetOf;
pub use slider::Slider;
pub use slider::SliderConfigError;
pub use slider::SliderDirection;
pub use slider::SliderRange;
pub use slider::SliderStep;

use crate::PanelSystems;
use crate::cascade;

/// Named scheduling points for semantic widget work.
#[derive(SystemSet, Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(crate) enum WidgetSystems {
    /// Reifies widget identities from the latest computed panel output.
    Reify,
    /// Applies widget creation and synchronization commands.
    ReifyCommandsApplied,
    /// Synchronizes the final disabled marker from the resolved cascade value.
    ResolveInteractivity,
}

/// Installs headless panel widget identity and reification.
pub(crate) struct WidgetsPlugin;

impl Plugin for WidgetsPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(cascade::cascade_plugin::<WidgetInteractivity>())
            .configure_sets(
                Update,
                (
                    WidgetSystems::Reify.after(PanelSystems::ComputeLayout),
                    WidgetSystems::ReifyCommandsApplied.after(WidgetSystems::Reify),
                    WidgetSystems::ResolveInteractivity
                        .after(WidgetSystems::ReifyCommandsApplied)
                        .before(PanelSystems::ResolvePanelAttachments),
                ),
            )
            .add_systems(
                Update,
                (
                    reify::reify_widgets.in_set(WidgetSystems::Reify),
                    ApplyDeferred.in_set(WidgetSystems::ReifyCommandsApplied),
                    interactivity::resolve_interactivity
                        .in_set(WidgetSystems::ResolveInteractivity),
                ),
            );
    }
}
