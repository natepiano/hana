//! Debug overlay system for fit target debugging.
//!
//! Provides retained screen-aligned boundary box, silhouette polygon, margin
//! line, and label visualization for the current camera fit target.

mod constants;
mod context;
mod convex_hull;
mod fit_target_bounds;
mod frame;
mod labels;
mod lines;
mod reconciliation;
mod screen_space;
mod visual;

use bevy::asset::AssetServer;
use bevy::camera::visibility::VisibilitySystems;
use bevy::pbr::MaterialPlugin;
use bevy::prelude::*;
use bevy::transform::TransformSystems;
pub use fit_target_bounds::FitTargetOverlayConfig;
use lines::FitOverlayLineMaterial;
use lines::FitOverlayLineMaterials;

use super::components::FitOverlay;

/// System set for resolving and reconciling fit-overlay visuals.
#[derive(SystemSet, Debug, Hash, PartialEq, Eq, Clone)]
pub(crate) struct FitOverlaySystemSet;

/// Plugin that enables fit target debug visualization.
pub(crate) struct ZoomOverlayPlugin;

impl Plugin for ZoomOverlayPlugin {
    fn build(&self, app: &mut App) {
        if app.world().contains_resource::<AssetServer>() {
            app.add_plugins(MaterialPlugin::<lines::FitOverlayLineMaterial>::default());
        } else {
            app.init_resource::<Assets<FitOverlayLineMaterial>>();
        }

        app.init_resource::<FitTargetOverlayConfig>()
            .init_resource::<FitOverlayLineMaterials>()
            .add_observer(fit_target_bounds::on_remove_fit_visualization)
            .configure_sets(
                PostUpdate,
                FitOverlaySystemSet
                    .after(TransformSystems::Propagate)
                    .before(VisibilitySystems::VisibilityPropagate)
                    .before(VisibilitySystems::CheckVisibility),
            )
            .add_systems(
                PostUpdate,
                (
                    reconciliation::deduplicate_fit_overlay_visuals,
                    fit_target_bounds::draw_fit_target_bounds,
                )
                    .chain()
                    .in_set(FitOverlaySystemSet)
                    .run_if(any_with_component::<FitOverlay>),
            )
            .add_systems(
                PostUpdate,
                reconciliation::cleanup_orphan_fit_overlay_visuals.in_set(FitOverlaySystemSet),
            );
    }
}
