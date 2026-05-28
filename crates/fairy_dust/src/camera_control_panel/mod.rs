//! Capability: a singleton screen-space `bevy_diegetic` camera control panel
//! that follows the currently-routed `bevy_lagrange::OrbitCam`.
//!
//! Single-camera examples and multi-camera examples both produce exactly one
//! panel at the bottom-right; its contents swap to reflect whichever camera
//! the cursor is currently routing input to (per `ResolvedOrbitCamInputRoute`).
//! Cameras may optionally carry a `Name` component — when present, it drives
//! the `CAMERA: <name>` title.

mod constants;
mod display;
mod guidance;
mod layout;
mod preset_switch;
mod snapshot;

use bevy::picking::mesh_picking::MeshPickingPlugin;
use bevy::prelude::*;
use bevy_diegetic::Anchor;
use bevy_diegetic::DiegeticPanel;
use bevy_diegetic::DiegeticPanelCommands;
use bevy_diegetic::DiegeticUiPlugin;
use bevy_diegetic::Fit;
use bevy_lagrange::OrbitCamInputMode;
use bevy_lagrange::OrbitCamInteractionEnded;
use bevy_lagrange::OrbitCamInteractionSourcesChanged;
use bevy_lagrange::OrbitCamInteractionStarted;
use bevy_lagrange::OrbitCamInteractionState;
use bevy_lagrange::OrbitCamPreset;
use bevy_lagrange::ResolvedOrbitCamInputRoute;
use display::CameraGuidanceDisplay;
use display::CameraGuidanceDisplayState;
use display::RenderState;
pub use guidance::CameraGuidance;
pub use guidance::CameraGuidanceRow;
pub use guidance::SourceVisibility;
use layout::build_guidance_tree;
use layout::unlit_panel_material;
use snapshot::CameraGuidanceSnapshot;
use snapshot::resolve_guidance_snapshot;

use crate::constants::INNER_BACKGROUND;
use crate::ensure_plugin;

/// Singleton marker for the camera control panel. `bound_camera` records
/// which `OrbitCam` the panel is currently showing — updated each frame from
/// `ResolvedOrbitCamInputRoute`.
#[derive(Component)]
pub(crate) struct CameraGuidancePanel {
    bound_camera: Option<Entity>,
}

/// Inner background color for the camera control panel. Defaults to the
/// crate's `INNER_BACKGROUND` constant; override via
/// [`SprinkleBuilder::with_camera_control_panel_background_color`].
#[derive(Resource, Clone, Copy, Debug)]
pub(crate) struct CameraControlPanelBackground(pub Color);

impl Default for CameraControlPanelBackground {
    fn default() -> Self { Self(INNER_BACKGROUND) }
}

pub(crate) fn install(app: &mut App) {
    ensure_panel_plugins(app);
    preset_switch::install(app);
    app.init_resource::<CameraControlPanelBackground>();
    app.add_systems(Startup, spawn_panel);
    app.add_systems(
        PostUpdate,
        (rebind_panel_on_route_change, repaint_panel_display),
    );
    app.add_observer(refresh_on_interaction_started)
        .add_observer(refresh_on_interaction_ended)
        .add_observer(refresh_on_sources_changed);
}

fn ensure_panel_plugins(app: &mut App) {
    ensure_plugin(app, DiegeticUiPlugin);
    ensure_plugin(app, MeshPickingPlugin);
}

fn spawn_panel(mut commands: Commands, background: Res<CameraControlPanelBackground>) {
    let snapshot = default_snapshot();
    let display = CameraGuidanceDisplay::default();
    let unlit = unlit_panel_material();
    let panel = DiegeticPanel::screen()
        .size(Fit, Fit)
        .anchor(Anchor::BottomRight)
        .material(unlit.clone())
        .text_material(unlit)
        .with_tree(build_guidance_tree(&snapshot, display, background.0))
        .build();

    match panel {
        Ok(panel) => {
            commands.spawn((
                CameraGuidancePanel { bound_camera: None },
                snapshot,
                CameraGuidanceDisplayState::from_display(display),
                panel,
                Transform::default(),
            ));
        },
        Err(error) => {
            error!("fairy_dust: failed to build camera control panel: {error}");
        },
    }
}

/// Placeholder snapshot rendered until the first route resolution completes.
fn default_snapshot() -> CameraGuidanceSnapshot {
    resolve_guidance_snapshot(
        None,
        None,
        Some(&OrbitCamInputMode::Preset(OrbitCamPreset::BlenderLike)),
    )
}

/// Rebuilds the panel snapshot when the routed camera changes, or when the
/// currently-bound camera's input-mode components change.
fn rebind_panel_on_route_change(
    mut commands: Commands,
    route: Res<ResolvedOrbitCamInputRoute>,
    cameras: Query<(
        Option<&Name>,
        Option<&CameraGuidance>,
        Option<&OrbitCamInteractionState>,
        Option<&OrbitCamInputMode>,
    )>,
    changed_cameras: Query<Entity, Or<(Changed<CameraGuidance>, Changed<OrbitCamInputMode>)>>,
    panel: Single<(
        Entity,
        &mut CameraGuidancePanel,
        &mut CameraGuidanceDisplayState,
    )>,
    background: Res<CameraControlPanelBackground>,
) {
    let routed = route.routed_camera();
    let (panel_entity, mut panel_marker, mut display_state) = panel.into_inner();

    let route_changed = panel_marker.bound_camera != routed;
    let data_changed = routed.is_some_and(|cam| changed_cameras.get(cam).is_ok());
    if !route_changed && !data_changed {
        return;
    }

    panel_marker.bound_camera = routed;

    let Some(cam) = routed else {
        return;
    };
    let Ok((name, guidance, state, mode)) = cameras.get(cam) else {
        return;
    };

    let snapshot = resolve_guidance_snapshot(name, guidance, mode);
    let display = CameraGuidanceDisplay::from_interaction_state(state.copied().unwrap_or_default());
    *display_state = CameraGuidanceDisplayState::from_display(display);
    commands.entity(panel_entity).insert(snapshot.clone());
    commands.set_tree(
        panel_entity,
        build_guidance_tree(&snapshot, display, background.0),
    );
}

fn refresh_on_interaction_started(
    event: On<OrbitCamInteractionStarted>,
    time: Res<Time<Real>>,
    panel: Single<(&CameraGuidancePanel, &mut CameraGuidanceDisplayState)>,
) {
    let (panel_marker, mut display) = panel.into_inner();
    if panel_marker.bound_camera != Some(event.camera) {
        return;
    }
    display.activate(event.kind, event.sources, time.elapsed_secs());
}

fn refresh_on_interaction_ended(
    event: On<OrbitCamInteractionEnded>,
    time: Res<Time<Real>>,
    panel: Single<(&CameraGuidancePanel, &mut CameraGuidanceDisplayState)>,
) {
    let (panel_marker, mut display) = panel.into_inner();
    if panel_marker.bound_camera != Some(event.camera) {
        return;
    }
    display.hold(event.kind, event.sources, time.elapsed_secs());
}

fn refresh_on_sources_changed(
    event: On<OrbitCamInteractionSourcesChanged>,
    time: Res<Time<Real>>,
    panel: Single<(&CameraGuidancePanel, &mut CameraGuidanceDisplayState)>,
) {
    let (panel_marker, mut display) = panel.into_inner();
    if panel_marker.bound_camera != Some(event.camera) {
        return;
    }
    display.activate(event.kind, event.current, time.elapsed_secs());
}

/// Expires held source labels and rebuilds the panel tree when display state
/// changes between frames.
fn repaint_panel_display(
    time: Res<Time<Real>>,
    mut commands: Commands,
    panel: Single<(
        Entity,
        &CameraGuidanceSnapshot,
        &mut CameraGuidanceDisplayState,
    )>,
    background: Res<CameraControlPanelBackground>,
) {
    let (panel_entity, snapshot, mut display) = panel.into_inner();
    display.expire_held_sources(time.elapsed_secs());
    if display.render_state == RenderState::Idle {
        return;
    }
    commands.set_tree(
        panel_entity,
        build_guidance_tree(snapshot, display.display(), background.0),
    );
    display.render_state = RenderState::Idle;
}
