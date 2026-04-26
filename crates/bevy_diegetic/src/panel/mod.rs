//! Panel integration: components, layout computation, gizmo rendering,
//! and OIT setup — the Bevy-facing half of diegetic UI.

mod builder;
mod compute_layout;
mod coordinate_space;
mod diegetic_panel;
mod gizmos;
mod perf;
mod sizing;

use bevy::prelude::*;
pub use builder::DiegeticPanelBuilder;
pub use coordinate_space::CoordinateSpace;
pub use coordinate_space::HueOffset;
pub use coordinate_space::RenderMode;
pub use coordinate_space::ScreenPosition;
pub use coordinate_space::SurfaceShadow;
pub use diegetic_panel::ComputedDiegeticPanel;
pub use diegetic_panel::DiegeticPanel;
pub(crate) use diegetic_panel::PanelFontUnit;
pub use gizmos::DiegeticPanelGizmoGroup;
pub use gizmos::ShowTextGizmos;
pub use perf::AtlasPerfStats;
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
use crate::cascade::CascadePanelPlugin;
use crate::layout::ShapedTextCache;

/// System sets for ordering panel work and its cross-module dependencies.
///
/// Other plugins (e.g., [`ScreenSpacePlugin`](crate::screen_space::ScreenSpacePlugin))
/// use `.before(PanelSystems::ComputeLayout)` / `.after(PanelSystems::ComputeLayout)`
/// rather than referencing concrete system symbols.
#[derive(SystemSet, Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(crate) enum PanelSystems {
    /// Runs [`compute_panel_layouts`](compute_layout::compute_panel_layouts).
    ComputeLayout,
    /// Runs [`resolve_world_panel_fit`](compute_layout::resolve_world_panel_fit)
    /// — shrinks world panels with `Fit` axes to their content bounds.
    ResolveWorldFit,
    /// Runs gizmo reconciliation
    /// ([`render_layout_gizmos`](gizmos::render_layout_gizmos) +
    /// [`render_debug_gizmos`](gizmos::render_debug_gizmos)).
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
/// and optionally override [`CascadeDefaults`] before adding this plugin.
pub struct HeadlessLayoutPlugin;

impl Plugin for HeadlessLayoutPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(perf::DiagnosticsPlugin)
            .add_plugins(CascadePanelPlugin::<PanelFontUnit>::default())
            .init_resource::<DiegeticPerfStats>()
            .init_resource::<ShapedTextCache>()
            .init_resource::<CascadeDefaults>()
            .configure_sets(
                Update,
                PanelSystems::ResolveWorldFit.after(PanelSystems::ComputeLayout),
            )
            .add_systems(
                Update,
                (
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
                (
                    gizmos::render_layout_gizmos.in_set(PanelSystems::RenderGizmos),
                    gizmos::render_debug_gizmos.in_set(PanelSystems::RenderGizmos),
                ),
            );
    }
}
