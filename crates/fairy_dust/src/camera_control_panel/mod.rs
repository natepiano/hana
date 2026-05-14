//! Capability: screen-space `bevy_diegetic` camera control panels for
//! `bevy_lagrange::OrbitCam` examples.

use bevy::picking::mesh_picking::MeshPickingPlugin;
use bevy::prelude::*;
use bevy_diegetic::DiegeticPanel;
use bevy_diegetic::DiegeticPanelCommands;
use bevy_diegetic::DiegeticUiPlugin;
use bevy_diegetic::Fit;
use bevy_lagrange::OrbitCam;
use bevy_lagrange::OrbitCamBindings;
use bevy_lagrange::OrbitCamInteractionEnded;
use bevy_lagrange::OrbitCamInteractionSourcesChanged;
use bevy_lagrange::OrbitCamInteractionStarted;
use bevy_lagrange::OrbitCamInteractionState;
use bevy_lagrange::OrbitCamManual;
use bevy_lagrange::OrbitCamPreset;

use crate::ensure_plugin;

mod config;
mod display;
mod layout;
mod snapshot;

pub use config::CameraGuidance;
pub use config::CameraGuidanceRow;
use display::CameraGuidanceDisplay;
use display::CameraGuidanceDisplayState;
use layout::build_guidance_tree;
use layout::unlit_panel_material;
use snapshot::CameraGuidanceSnapshot;
use snapshot::resolve_guidance_snapshot;

#[derive(Component)]
struct CameraGuidancePanel {
    camera: Entity,
}

pub(crate) fn install(app: &mut App) {
    ensure_panel_plugins(app);
    app.add_systems(
        PostUpdate,
        (refresh_changed_guidance_snapshot, refresh_guidance_display),
    );
    app.add_observer(attach_default_guidance_on_orbit_cam_add)
        .add_observer(spawn_guidance_panel_on_add)
        .add_observer(refresh_on_interaction_started)
        .add_observer(refresh_on_interaction_ended)
        .add_observer(refresh_on_sources_changed);
}

fn ensure_panel_plugins(app: &mut App) {
    ensure_plugin(app, DiegeticUiPlugin);
    ensure_plugin(app, MeshPickingPlugin);
}

fn attach_default_guidance_on_orbit_cam_add(
    trigger: On<Add, OrbitCam>,
    mut commands: Commands,
    cameras: Query<(), (With<OrbitCam>, Without<CameraGuidance>)>,
) {
    let camera = trigger.entity;
    if cameras.get(camera).is_ok() {
        commands.entity(camera).insert(CameraGuidance::auto());
    }
}

fn spawn_guidance_panel_on_add(
    trigger: On<Add, CameraGuidance>,
    mut commands: Commands,
    cameras: Query<(
        &CameraGuidance,
        Option<&OrbitCamInteractionState>,
        Option<&OrbitCamPreset>,
        Option<&OrbitCamBindings>,
        Option<&OrbitCamManual>,
    )>,
) {
    let camera = trigger.entity;
    let Ok((guidance, state, preset, bindings, manual)) = cameras.get(camera) else {
        return;
    };
    let snapshot = resolve_guidance_snapshot(guidance, preset, bindings, manual);
    let display = CameraGuidanceDisplay::from_interaction_state(state.copied().unwrap_or_default());
    let unlit = unlit_panel_material();
    let panel = DiegeticPanel::screen()
        .size(Fit, Fit)
        .anchor(guidance.anchor)
        .material(unlit.clone())
        .text_material(unlit)
        .with_tree(build_guidance_tree(&snapshot, display))
        .build();

    match panel {
        Ok(panel) => {
            commands.spawn((
                CameraGuidancePanel { camera },
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

fn refresh_changed_guidance_snapshot(
    mut commands: Commands,
    cameras: Query<
        (
            Entity,
            &CameraGuidance,
            Option<&OrbitCamInteractionState>,
            Option<&OrbitCamPreset>,
            Option<&OrbitCamBindings>,
            Option<&OrbitCamManual>,
        ),
        Or<(
            Changed<CameraGuidance>,
            Changed<OrbitCamPreset>,
            Changed<OrbitCamBindings>,
            Changed<OrbitCamManual>,
        )>,
    >,
    mut panels: Query<(
        Entity,
        &CameraGuidancePanel,
        &mut CameraGuidanceDisplayState,
    )>,
) {
    for (camera, guidance, state, preset, bindings, manual) in &cameras {
        let snapshot = resolve_guidance_snapshot(guidance, preset, bindings, manual);
        let display =
            CameraGuidanceDisplay::from_interaction_state(state.copied().unwrap_or_default());
        refresh_camera_guidance_snapshot(camera, snapshot, display, &mut commands, &mut panels);
    }
}

fn refresh_on_interaction_started(
    event: On<OrbitCamInteractionStarted>,
    time: Res<Time<Real>>,
    mut panels: Query<(&CameraGuidancePanel, &mut CameraGuidanceDisplayState)>,
) {
    update_camera_guidance_display(event.camera, &mut panels, |display| {
        display.activate(event.kind, event.sources, time.elapsed_secs());
    });
}

fn refresh_on_interaction_ended(
    event: On<OrbitCamInteractionEnded>,
    time: Res<Time<Real>>,
    mut panels: Query<(&CameraGuidancePanel, &mut CameraGuidanceDisplayState)>,
) {
    update_camera_guidance_display(event.camera, &mut panels, |display| {
        display.hold(event.kind, event.sources, time.elapsed_secs());
    });
}

fn refresh_on_sources_changed(
    event: On<OrbitCamInteractionSourcesChanged>,
    time: Res<Time<Real>>,
    mut panels: Query<(&CameraGuidancePanel, &mut CameraGuidanceDisplayState)>,
) {
    update_camera_guidance_display(event.camera, &mut panels, |display| {
        display.activate(event.kind, event.current, time.elapsed_secs());
    });
}

fn update_camera_guidance_display(
    camera: Entity,
    panels: &mut Query<(&CameraGuidancePanel, &mut CameraGuidanceDisplayState)>,
    update: impl Fn(&mut CameraGuidanceDisplayState),
) {
    panels
        .iter_mut()
        .filter(|(panel_camera, _)| panel_camera.camera == camera)
        .for_each(|(_, mut display)| update(&mut display));
}

fn refresh_camera_guidance_snapshot(
    camera: Entity,
    snapshot: CameraGuidanceSnapshot,
    display: CameraGuidanceDisplay,
    commands: &mut Commands,
    panels: &mut Query<(
        Entity,
        &CameraGuidancePanel,
        &mut CameraGuidanceDisplayState,
    )>,
) {
    for (panel, panel_camera, mut display_state) in panels.iter_mut() {
        if panel_camera.camera == camera {
            commands.entity(panel).insert(snapshot.clone());
            *display_state = CameraGuidanceDisplayState::from_display(display);
            commands.set_tree(panel, build_guidance_tree(&snapshot, display));
        }
    }
}

fn refresh_guidance_display(
    time: Res<Time<Real>>,
    mut commands: Commands,
    mut panels: Query<(
        Entity,
        &CameraGuidanceSnapshot,
        &mut CameraGuidanceDisplayState,
    )>,
) {
    for (panel, snapshot, mut display) in &mut panels {
        display.expire_held_sources(time.elapsed_secs());
        if !display.needs_render {
            continue;
        }

        commands.set_tree(panel, build_guidance_tree(snapshot, display.display()));
        display.needs_render = false;
    }
}
