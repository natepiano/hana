//! Panel integration: components, layout computation, and gizmo rendering
//! — the Bevy-facing half of diegetic UI.

mod anchor_geometry;
mod anchoring;
mod attachment_resolver;
mod builder;
mod compute_layout;
mod constants;
mod conversion;
mod coordinate_space;
mod diegetic_panel;
mod events;
mod field;
mod gizmos;
mod perf;
mod sizing;
mod world_anchoring;

pub use anchor_geometry::PanelAnchorEdge;
pub use anchor_geometry::PanelAnchorEdgeEndpoints;
pub use anchor_geometry::PanelAnchorGeometryError;
pub use anchor_geometry::PanelAnchorGeometryParam;
pub use anchor_geometry::PanelAnchorPoint;
pub use anchor_geometry::PanelAnchorPoints;
pub use anchor_geometry::PanelPlane;
pub use anchor_geometry::PanelScreenBounds;
pub use anchor_geometry::ResolvedPanelAnchorGeometry;
pub(crate) use anchor_geometry::screen_anchor_position;
pub use anchoring::AnchoredToPanel;
pub use anchoring::PanelAnchorOffset;
pub use anchoring::PanelAnchorPose;
pub use anchoring::PanelsAnchoredHere;
pub(crate) use anchoring::ResolvedScreenPanelPosition;
pub(crate) use attachment_resolver::AttachmentResolveAction;
pub(crate) use attachment_resolver::AttachmentResolveCandidate;
pub(crate) use attachment_resolver::AttachmentResolveDiagnostics;
pub(crate) use attachment_resolver::AttachmentResolveReasons;
pub(crate) use attachment_resolver::resolve_panel_attachments;
use bevy::ecs::schedule::ApplyDeferred;
use bevy::prelude::*;
use bevy::transform::TransformSystems;
pub use builder::DiegeticPanelBuilder;
pub use builder::PanelBuildError;
pub use conversion::PanelProjectionError;
pub use conversion::PanelProjectionParam;
pub use conversion::PanelScreenConversion;
pub use conversion::PanelScreenConversionParam;
pub use conversion::PanelScreenHandoff;
pub use conversion::PanelScreenProjection;
pub use conversion::PanelScreenTarget;
pub use conversion::PanelWorldConversion;
pub use conversion::PanelWorldConversionParam;
pub use conversion::PanelWorldProjection;
pub use conversion::PanelWorldTarget;
pub use conversion::SavedPanelScreenState;
pub use conversion::SavedPanelWorldState;
pub(crate) use conversion::apply_screen_conversion;
pub(crate) use conversion::apply_screen_root_sizing;
pub(crate) use conversion::apply_world_conversion;
pub(crate) use conversion::validate_screen_conversion;
pub(crate) use conversion::validate_world_conversion;
pub use coordinate_space::CoordinateSpace;
pub use coordinate_space::ScreenPosition;
pub use coordinate_space::SurfaceShadow;
pub use diegetic_panel::ComputedDiegeticPanel;
pub use diegetic_panel::DiegeticPanel;
pub(crate) use diegetic_panel::DiegeticPanelChangeClassification;
pub use diegetic_panel::DiegeticPanelCommands;
pub(crate) use events::LastPanelDimensions;
pub use events::PanelChangeKind;
pub use events::PanelChanged;
pub use events::PanelDimensions;
pub use events::PanelDimensionsChanged;
pub(crate) use events::trigger_panel_dimensions_changed;
pub use field::PanelFieldRecord;
pub use gizmos::DiegeticPanelGizmoGroup;
pub use gizmos::ShowTextGizmos;
pub use perf::BatchPerfStats;
use perf::DiagnosticsPlugin;
pub use perf::DiegeticPerfStats;
pub use perf::MaterialTablePerfStats;
pub use perf::PanelGeometryPerfStats;
pub use perf::PanelShapeBatchPerfStats;
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
use world_anchoring::WorldAnchorResolveDiagnostics;

use crate::cascade::CascadeDefaults;
use crate::cascade::CascadePlugin;
use crate::cascade::FontUnit;
use crate::layout::ShapedTextCache;
use crate::render::AntiAlias;
use crate::render::HairlineFade;

/// System sets for ordering panel work and its cross-module dependencies.
///
/// Other plugins (e.g., `ScreenSpacePlugin`)
/// use `.before(PanelSystems::ComputeLayout)` / `.after(PanelSystems::ComputeLayout)`
/// rather than referencing concrete system symbols.
#[derive(SystemSet, Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum PanelSystems {
    /// Applies deferred tree replacement commands before layout responds.
    ApplyTreeChanges,
    /// Applies queued coordinate-space conversions before layout responds.
    ApplyConversions,
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
    /// `PostUpdate` ordering point for animation systems that write
    /// resolver-read inputs — `PanelAnchorPose`, and relation insert/remove at
    /// state boundaries — before `resolve_world_space_panel_attachments`.
    /// Writes here land this frame; writes after the resolver land next frame.
    AnimateAnchorPose,
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
            // Anti-alias and hairline fade live here, not in `RenderPlugin`,
            // because `seed_panel_overrides` reads their `CascadeDefault<A>`
            // resources — headless layout apps must have them.
            .add_plugins(CascadePlugin::<AntiAlias>::default())
            .add_plugins(CascadePlugin::<HairlineFade>::default())
            .add_observer(diegetic_panel::seed_panel_overrides)
            .init_resource::<DiegeticPerfStats>()
            .init_resource::<ShapedTextCache>()
            .init_resource::<CascadeDefaults>()
            .init_resource::<WorldAnchorResolveDiagnostics>()
            .configure_sets(
                Update,
                (
                    PanelSystems::ApplyTreeChanges.before(PanelSystems::ApplyConversions),
                    PanelSystems::ApplyConversions.before(PanelSystems::ComputeLayout),
                    PanelSystems::ResolveWorldFit.after(PanelSystems::ComputeLayout),
                ),
            )
            .add_systems(
                Update,
                (
                    ApplyDeferred.in_set(PanelSystems::ApplyTreeChanges),
                    (
                        ApplyDeferred,
                        diegetic_panel::apply_pending_panel_conversions,
                        ApplyDeferred,
                    )
                        .chain()
                        .in_set(PanelSystems::ApplyConversions),
                    compute_layout::compute_panel_layouts.in_set(PanelSystems::ComputeLayout),
                    compute_layout::resolve_world_panel_fit.in_set(PanelSystems::ResolveWorldFit),
                ),
            )
            .configure_sets(
                PostUpdate,
                PanelSystems::AnimateAnchorPose
                    .before(world_anchoring::resolve_world_space_panel_attachments),
            )
            .add_systems(
                PostUpdate,
                (
                    world_anchoring::restore_inactive_world_panel_poses,
                    world_anchoring::resolve_world_space_panel_attachments,
                )
                    .chain()
                    .before(TransformSystems::Propagate),
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
