//! Panel integration: components, layout computation, and gizmo rendering
//! — the Bevy-facing half of diegetic UI.

mod anchoring;
mod builder;
mod compute_layout;
mod constants;
mod coordinate_space;
mod diegetic_panel;
mod events;
mod field;
mod gizmos;
mod perf;
mod sizing;

pub use anchoring::AnchoredToPanel;
pub use anchoring::PanelAnchorOffset;
pub use anchoring::PanelAnchorOffsetUnits;
pub use anchoring::PanelsAnchoredHere;
pub(crate) use anchoring::ResolvedScreenPanelPosition;
use bevy::ecs::schedule::ApplyDeferred;
use bevy::prelude::*;
pub use builder::DiegeticPanelBuilder;
pub use builder::PanelBuildError;
pub use coordinate_space::CoordinateSpace;
pub use coordinate_space::ScreenPosition;
pub use coordinate_space::SurfaceShadow;
pub use diegetic_panel::ComputedDiegeticPanel;
pub use diegetic_panel::DiegeticPanel;
pub(crate) use diegetic_panel::DiegeticPanelChangeClassification;
pub use diegetic_panel::DiegeticPanelCommands;
pub(crate) use events::LastPanelDimensions;
pub use events::PanelDimensions;
pub use events::PanelDimensionsChanged;
pub(crate) use events::trigger_panel_dimensions_changed;
pub use field::PanelFieldRecord;
pub use gizmos::DiegeticPanelGizmoGroup;
pub use gizmos::ShowTextGizmos;
pub use perf::BatchPerfStats;
use perf::DiagnosticsPlugin;
pub use perf::DiegeticPerfStats;
pub use perf::PanelTextPerfStats;
pub use sizing::AnyUnit;
pub use sizing::CompatibleUnits;
pub use sizing::Fit;
pub use sizing::FitMax;
pub use sizing::FitRange;
pub use sizing::Grow;
pub use sizing::GrowMax;
pub use sizing::GrowRange;
pub use sizing::Inches;
pub use sizing::Millimeters;
pub use sizing::PanelSizing;
pub use sizing::Percent;
pub use sizing::Pixels;
pub use sizing::Points;

use crate::cascade::CascadeDefaults;
use crate::cascade::CascadePlugin;
use crate::cascade::FontUnit;
use crate::layout::ShapedTextCache;

/// System sets for ordering panel work and its cross-module dependencies.
///
/// Other plugins (e.g., `ScreenSpacePlugin`)
/// use `.before(PanelSystems::ComputeLayout)` / `.after(PanelSystems::ComputeLayout)`
/// rather than referencing concrete system symbols.
#[derive(SystemSet, Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum PanelSystems {
    /// Applies deferred tree replacement commands before layout responds.
    ApplyTreeChanges,
    /// Runs `compute_panel_layouts`.
    ComputeLayout,
    /// Runs `resolve_world_panel_fit`
    /// — shrinks world panels with `Fit` axes to their content bounds.
    ResolveWorldFit,
    /// Resolves panel-to-panel attachments before screen-space positioning.
    ResolvePanelAttachments,
    /// Positions screen-space panels after `Fit` dimensions have resolved.
    PositionScreenSpace,
    /// Runs gizmo reconciliation
    /// (`render_debug_gizmos`).
    RenderGizmos,
}

/// Headless layout runner — schedules `compute_panel_layouts` on `Update`
/// and initializes the resources it consumes (diagnostics, perf stats,
/// shaped-text cache).
///
/// External consumers (benchmarks, non-UI apps) register this plugin
/// instead of [`DiegeticUiPlugin`](crate::DiegeticUiPlugin) when they
/// only need [`DiegeticPanel`] → [`ComputedDiegeticPanel`] computation.
/// The plugin initializes [`CascadeDefaults`] itself (idempotent); callers
/// insert their own [`DiegeticTextMeasurer`](crate::DiegeticTextMeasurer)
/// and optionally override construction defaults before adding this plugin.
pub struct HeadlessLayoutPlugin;

impl Plugin for HeadlessLayoutPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(DiagnosticsPlugin)
            .add_plugins(CascadePlugin::<FontUnit>::default())
            .add_observer(diegetic_panel::seed_panel_overrides)
            .init_resource::<DiegeticPerfStats>()
            .init_resource::<ShapedTextCache>()
            .init_resource::<CascadeDefaults>()
            .register_type::<AnchoredToPanel>()
            .register_type::<PanelAnchorOffset>()
            .register_type::<PanelAnchorOffsetUnits>()
            .register_type::<PanelsAnchoredHere>()
            .configure_sets(
                Update,
                (
                    PanelSystems::ApplyTreeChanges.before(PanelSystems::ComputeLayout),
                    PanelSystems::ResolveWorldFit.after(PanelSystems::ComputeLayout),
                ),
            )
            .add_systems(
                Update,
                (
                    ApplyDeferred.in_set(PanelSystems::ApplyTreeChanges),
                    compute_layout::compute_panel_layouts.in_set(PanelSystems::ComputeLayout),
                    compute_layout::resolve_world_panel_fit.in_set(PanelSystems::ResolveWorldFit),
                ),
            );
    }
}

/// Full panel integration — headless layout plus gizmo debug rendering.
/// Registered by [`DiegeticUiPlugin`](crate::DiegeticUiPlugin).
pub(crate) struct PanelPlugin;

impl Plugin for PanelPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(HeadlessLayoutPlugin)
            .init_resource::<ShowTextGizmos>()
            .init_gizmo_group::<DiegeticPanelGizmoGroup>()
            .configure_sets(
                Update,
                PanelSystems::RenderGizmos.after(PanelSystems::ResolveWorldFit),
            )
            .add_systems(Startup, gizmos::configure_panel_gizmos)
            .add_systems(
                Update,
                gizmos::render_debug_gizmos.in_set(PanelSystems::RenderGizmos),
            );
    }
}
