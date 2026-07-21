mod button;
mod focus;
mod id;
mod input;
mod interactivity;
mod picking;
mod reify;
mod relationship;
mod slider;

use bevy::ecs::schedule::ApplyDeferred;
use bevy::ecs::schedule::common_conditions::resource_exists;
use bevy::picking::PickingSystems;
use bevy::picking::mesh_picking::MeshPickingSettings;
use bevy::prelude::*;
use bevy::window::WindowFocused;
pub use button::Button;
pub use focus::ClearWidgetFocus;
pub use focus::RequestWidgetFocus;
pub(crate) use focus::WidgetFocusAuthority;
pub use focus::WidgetFocusChangeCause;
pub use focus::WidgetFocusChanged;
pub use focus::WidgetFocusable;
pub use focus::WidgetFocused;
pub(crate) use focus::finalize_panel_focus;
use hana_valence::AnchorSystems;
pub(crate) use id::ComputedWidgetRecord;
pub use id::PanelWidget;
pub(crate) use id::PanelWidgetIndex;
pub use id::PanelWidgetReader;
pub(crate) use id::WidgetKind;
pub(crate) use id::WidgetSpec;
pub(crate) use id::validate_tree;
pub use input::ActivateFocusedWidget;
pub use input::CancelFocusedWidget;
pub use input::FocusFirstWidget;
pub use input::FocusLastWidget;
pub use input::FocusNextWidget;
pub use input::FocusPreviousWidget;
pub use input::WidgetControlSummary;
pub use input::WidgetInputBindings;
pub use input::WidgetInputBindingsBuilder;
pub use input::WidgetInputBindingsError;
pub use input::WidgetInputDisabled;
pub use input::WidgetInputMode;
pub use input::WidgetInputPlugin;
pub use interactivity::PanelWidgetWriter;
pub use interactivity::WidgetDisabled;
pub use interactivity::WidgetInteractivity;
pub(crate) use reify::WidgetAnchorRect;
pub(crate) use reify::on_screen_widget_demand_added;
pub(crate) use reify::on_screen_widget_demand_removed;
pub(crate) use reify::update_screen_anchor_geometry;
pub use relationship::PanelWidgets;
pub(crate) use relationship::ScreenWidgetAnchorProxy;
pub(crate) use relationship::ScreenWidgetAnchoredHere;
pub(crate) use relationship::ScreenWidgetAnchoredTo;
pub use relationship::WidgetOf;
pub use slider::Slider;
pub use slider::SliderConfigError;
pub use slider::SliderDirection;
pub use slider::SliderRange;
pub use slider::SliderStep;

use crate::PanelSystems;
use crate::cascade;
use crate::ime::ImeSystemSet;
use crate::screen_space::ScreenSpaceSystems;

/// Named scheduling points for semantic widget work.
#[derive(SystemSet, Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(crate) enum WidgetSystems {
    /// Reifies widget identities from the latest computed panel output.
    Reify,
    /// Applies widget creation and synchronization commands.
    ReifyCommandsApplied,
    /// Synchronizes the final disabled marker from the resolved cascade value.
    ResolveInteractivity,
    /// Applies commands that synchronize the disabled marker.
    InteractivityCommandsApplied,
    /// Reconciles window-scoped focus after widget and window removals.
    Focus,
    /// Routes window-scoped semantic requests to focus or widget intents.
    SemanticInput,
}

/// Installs headless panel widget identity and reification.
pub(crate) struct WidgetsPlugin;

impl Plugin for WidgetsPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<MeshPickingSettings>()
            .init_resource::<WidgetFocusAuthority>()
            .add_message::<WindowFocused>()
            .add_message::<FocusNextWidget>()
            .add_message::<FocusPreviousWidget>()
            .add_message::<FocusFirstWidget>()
            .add_message::<FocusLastWidget>()
            .add_message::<ActivateFocusedWidget>()
            .add_message::<CancelFocusedWidget>()
            .add_plugins(cascade::cascade_plugin::<WidgetInteractivity>())
            .configure_sets(
                Update,
                (
                    WidgetSystems::Reify
                        .after(PanelSystems::ComputeLayout)
                        .after(PanelSystems::ResolveWorldFit)
                        .after(ScreenSpaceSystems::ResolveDimensions),
                    WidgetSystems::ReifyCommandsApplied.after(WidgetSystems::Reify),
                    WidgetSystems::ResolveInteractivity
                        .after(WidgetSystems::ReifyCommandsApplied)
                        .before(WidgetSystems::InteractivityCommandsApplied),
                    WidgetSystems::InteractivityCommandsApplied
                        .after(WidgetSystems::ResolveInteractivity)
                        .before(WidgetSystems::Focus),
                    WidgetSystems::Focus
                        .after(ImeSystemSet::PublishInputBlockers)
                        .before(WidgetSystems::SemanticInput),
                    WidgetSystems::SemanticInput
                        .after(WidgetSystems::Focus)
                        .before(PanelSystems::ResolvePanelAttachments),
                ),
            )
            .add_systems(
                PreUpdate,
                picking::update_hits
                    .run_if(
                        resource_exists::<bevy::picking::backend::ray::RayMap>
                            .and_then(resource_exists::<Assets<Mesh>>),
                    )
                    .in_set(PickingSystems::Backend),
            )
            .add_observer(focus::request_widget_focus)
            .add_observer(focus::clear_widget_focus)
            .add_observer(focus::focus_from_pointer_press)
            .add_systems(
                Update,
                (
                    reify::reify_widgets.in_set(WidgetSystems::Reify),
                    ApplyDeferred.in_set(WidgetSystems::ReifyCommandsApplied),
                    interactivity::resolve_interactivity
                        .in_set(WidgetSystems::ResolveInteractivity),
                    ApplyDeferred.in_set(WidgetSystems::InteractivityCommandsApplied),
                    focus::cleanup_removed_focus_participants.in_set(WidgetSystems::Focus),
                    input::route_semantic_input.in_set(WidgetSystems::SemanticInput),
                ),
            )
            .add_systems(
                PostUpdate,
                reify::update_world_anchor_geometry.in_set(AnchorSystems::FillGeometry),
            );
    }
}
