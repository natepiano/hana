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
use bevy_diegetic::PanelChanged;
use bevy_lagrange::CameraInteractionSources;
use bevy_lagrange::OrbitCamInputMode;
use bevy_lagrange::OrbitCamInteractionEnded;
use bevy_lagrange::OrbitCamInteractionSourcesChanged;
use bevy_lagrange::OrbitCamInteractionSpeedChanged;
use bevy_lagrange::OrbitCamInteractionStarted;
use bevy_lagrange::OrbitCamInteractionState;
use bevy_lagrange::OrbitCamSlowModeState;
use bevy_lagrange::ResolvedOrbitCamInputRoute;
use display::CameraGuidanceDisplay;
use display::CameraGuidanceDisplayState;
use display::RenderState;
pub use guidance::CameraGuidance;
pub use guidance::CameraGuidanceRow;
use layout::build_guidance_tree;
pub(crate) use preset_switch::CameraPresetSwitching;
use snapshot::CameraGuidanceSnapshot;
use snapshot::resolve_guidance_snapshot;

use crate::constants::INNER_BACKGROUND;
use crate::ensure_plugin;
use crate::screen_panels;

/// Singleton marker for the camera control panel. `bound_camera` records
/// which `OrbitCam` the panel is currently showing — updated each frame from
/// `ResolvedOrbitCamInputRoute`.
#[derive(Component)]
pub(crate) struct CameraGuidancePanel {
    bound_camera: Option<Entity>,
}

#[derive(Component)]
struct CameraGuidanceRevealPending;

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
        (
            rebind_panel_on_route_change,
            track_slow_mode_state,
            track_live_zoom_direction,
            tick_highlight_release,
            repaint_panel_display,
        )
            .chain(),
    );
    app.add_observer(refresh_on_interaction_started)
        .add_observer(refresh_on_interaction_ended)
        .add_observer(refresh_on_sources_changed)
        .add_observer(refresh_on_speed_changed);
}

fn ensure_panel_plugins(app: &mut App) {
    ensure_plugin(app, DiegeticUiPlugin);
    ensure_plugin(app, MeshPickingPlugin);
}

fn spawn_panel(mut commands: Commands, background: Res<CameraControlPanelBackground>) {
    let snapshot = default_snapshot();
    let display = CameraGuidanceDisplay::default();
    let unlit = screen_panels::screen_panel_material();
    let panel = DiegeticPanel::screen()
        .size(Fit, Fit)
        .anchor(Anchor::BottomRight)
        .material(unlit.clone())
        .text_material(unlit)
        .with_tree(build_guidance_tree(&snapshot, display, background.0))
        .build();

    match panel {
        Ok(panel) => {
            commands
                .spawn((
                    CameraGuidancePanel { bound_camera: None },
                    snapshot,
                    CameraGuidanceDisplayState::from_display(display),
                    panel,
                    Transform::default(),
                ))
                .observe(
                    reveal_panel_after_rebuild
                        .run_if(any_with_component::<CameraGuidanceRevealPending>),
                );
        },
        Err(error) => {
            error!("fairy_dust: failed to build camera control panel: {error}");
        },
    }
}

/// Placeholder snapshot rendered for the one frame before the panel binds to a
/// camera. Uses the default input mode so it never claims an unrelated preset.
fn default_snapshot() -> CameraGuidanceSnapshot {
    resolve_guidance_snapshot(None, None, Some(&OrbitCamInputMode::default()))
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
        Option<&OrbitCamSlowModeState>,
        Option<&OrbitCamInputMode>,
    )>,
    orbit_cameras: Query<Entity, With<OrbitCamInputMode>>,
    changed_cameras: Query<Entity, Or<(Changed<CameraGuidance>, Changed<OrbitCamInputMode>)>>,
    panel: Single<(
        Entity,
        &mut CameraGuidancePanel,
        &mut CameraGuidanceDisplayState,
    )>,
    background: Res<CameraControlPanelBackground>,
) {
    // Before the cursor routes to a camera, follow an orbit camera so the panel
    // shows a real preset instead of the fabricated default. With one camera
    // that is the only choice; with several, the lowest-entity camera (first
    // spawned) stands in until the cursor routes input to a specific one.
    let routed = route.routed_camera().or_else(|| orbit_cameras.iter().min());
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
    let Ok((name, guidance, state, slow_mode, mode)) = cameras.get(cam) else {
        return;
    };

    let snapshot = resolve_guidance_snapshot(name, guidance, mode);
    let display = CameraGuidanceDisplay::from_camera_state(
        state.copied().unwrap_or_default(),
        slow_mode.is_some_and(|state| state.is_active()),
    );
    *display_state = CameraGuidanceDisplayState::from_display(display);
    commands.entity(panel_entity).insert(snapshot.clone());
    commands.set_tree(
        panel_entity,
        build_guidance_tree(&snapshot, display, background.0),
    );
}

fn refresh_on_interaction_started(
    event: On<OrbitCamInteractionStarted>,
    panel: Single<(&CameraGuidancePanel, &mut CameraGuidanceDisplayState)>,
) {
    let (panel_marker, mut display) = panel.into_inner();
    if panel_marker.bound_camera != Some(event.camera) {
        return;
    }
    display.set_sources(event.kind, event.sources);
    // A fresh interaction's speed is unsettled until lagrange reports it, so the
    // singular variant does not flash before a slow-gate chord registers.
    display.set_speed(event.kind, None);
}

fn refresh_on_interaction_ended(
    event: On<OrbitCamInteractionEnded>,
    panel: Single<(&CameraGuidancePanel, &mut CameraGuidanceDisplayState)>,
) {
    let (panel_marker, mut display) = panel.into_inner();
    if panel_marker.bound_camera != Some(event.camera) {
        return;
    }
    display.set_sources(event.kind, CameraInteractionSources::NONE);
}

fn refresh_on_sources_changed(
    event: On<OrbitCamInteractionSourcesChanged>,
    panel: Single<(&CameraGuidancePanel, &mut CameraGuidanceDisplayState)>,
) {
    let (panel_marker, mut display) = panel.into_inner();
    if panel_marker.bound_camera != Some(event.camera) {
        return;
    }
    display.set_sources(event.kind, event.current);
}

fn track_slow_mode_state(
    cameras: Query<&OrbitCamSlowModeState>,
    panel: Single<(&CameraGuidancePanel, &mut CameraGuidanceDisplayState)>,
) {
    let (panel_marker, mut display) = panel.into_inner();
    let Some(camera) = panel_marker.bound_camera else {
        return;
    };
    display.set_slow_mode_active(
        cameras
            .get(camera)
            .is_ok_and(|slow_mode| slow_mode.is_active()),
    );
}

/// Mirrors the bound camera's reported zoom direction onto the panel display
/// each frame. lagrange computes and holds the direction in
/// [`OrbitCamInteractionState`], so reversing direction (zoom-in to zoom-out)
/// flips the highlighted row at once without waiting on the source debounce.
fn track_live_zoom_direction(
    cameras: Query<&OrbitCamInteractionState>,
    panel: Single<(&CameraGuidancePanel, &mut CameraGuidanceDisplayState)>,
) {
    let (panel_marker, mut display) = panel.into_inner();
    let Some(camera) = panel_marker.bound_camera else {
        return;
    };
    if let Ok(state) = cameras.get(camera) {
        display.set_zoom_direction(state.zoom_direction());
    }
}

fn tick_highlight_release(time: Res<Time>, panel: Single<&mut CameraGuidanceDisplayState>) {
    panel.into_inner().tick_highlight_release(time.delta());
}

fn refresh_on_speed_changed(
    event: On<OrbitCamInteractionSpeedChanged>,
    panel: Single<(&CameraGuidancePanel, &mut CameraGuidanceDisplayState)>,
) {
    let (panel_marker, mut display) = panel.into_inner();
    if panel_marker.bound_camera != Some(event.camera) {
        return;
    }
    display.set_speed(event.kind, Some(event.speed));
}

/// Rebuilds the panel tree when the mirrored display state changes between
/// frames.
fn repaint_panel_display(
    mut commands: Commands,
    panel: Single<(
        Entity,
        &CameraGuidanceSnapshot,
        &mut CameraGuidanceDisplayState,
    )>,
    background: Res<CameraControlPanelBackground>,
) {
    let (panel_entity, snapshot, mut display) = panel.into_inner();
    if display.render_state == RenderState::Idle {
        return;
    }
    commands.set_tree(
        panel_entity,
        build_guidance_tree(snapshot, display.display(), background.0),
    );
    display.render_state = RenderState::Idle;
}

fn reveal_panel_after_rebuild(event: On<PanelChanged>, mut commands: Commands) {
    commands
        .entity(event.entity)
        .insert(Visibility::Inherited)
        .remove::<CameraGuidanceRevealPending>();
}
