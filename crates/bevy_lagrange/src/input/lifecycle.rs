use bevy::prelude::*;

use super::CameraInputMetricKind;
use super::CameraInputMetricsMissing;
use super::CameraInputSourceLatches;
use super::CameraInputSurfaceMetrics;
use super::CameraInteractionSources;
use super::OrbitCamInput;
use super::OrbitCamInputBlockers;
use super::OrbitCamInteractionEnded;
use super::OrbitCamInteractionKind;
use super::OrbitCamInteractionSourcesChanged;
use super::OrbitCamInteractionStarted;
use super::OrbitCamInteractionState;
use super::ResolvedOrbitCamInputRoute;
use crate::system_sets::OrbitCamInputPhase;

#[derive(Clone, Copy, Debug)]
enum LifecycleEvent {
    Started(OrbitCamInteractionStarted),
    Ended(OrbitCamInteractionEnded),
    SourcesChanged(OrbitCamInteractionSourcesChanged),
    MetricsMissing(CameraInputMetricsMissing),
}

#[derive(Clone, Copy, Debug)]
enum InputMetricStatus {
    Complete,
    Missing,
}

#[derive(Clone, Debug)]
struct FinalizedInput {
    camera: Entity,
    input:  OrbitCamInput,
    state:  OrbitCamInteractionState,
    events: Vec<LifecycleEvent>,
}

pub(crate) struct OrbitCamInputLifecyclePlugin;

impl Plugin for OrbitCamInputLifecyclePlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            PreUpdate,
            finalize_orbit_cam_input.in_set(OrbitCamInputPhase::Finalize),
        );
    }
}

fn finalize_orbit_cam_input(world: &mut World) {
    let route = world.get_resource::<ResolvedOrbitCamInputRoute>().cloned();
    let mut query = world.query::<(
        Entity,
        &OrbitCamInput,
        Option<&OrbitCamInteractionState>,
        Option<&OrbitCamInputBlockers>,
        Option<&CameraInputSurfaceMetrics>,
    )>();
    let finalized = query
        .iter(world)
        .map(|(camera, input, state, blockers, metrics)| {
            finalize_camera_input(
                camera,
                *input,
                state.copied().unwrap_or_default(),
                blockers.copied().unwrap_or_default(),
                merged_surface_metrics(
                    route.as_ref().and_then(|route| route.metrics_for(camera)),
                    metrics.copied(),
                ),
            )
        })
        .collect::<Vec<_>>();

    for finalized in finalized {
        if let Some(mut input) = world.get_mut::<OrbitCamInput>(finalized.camera) {
            *input = finalized.input;
        }
        world.entity_mut(finalized.camera).insert(finalized.state);
        for event in finalized.events {
            apply_lifecycle_event(world, finalized.camera, event);
        }
    }
}

const fn merged_surface_metrics(
    routed: Option<CameraInputSurfaceMetrics>,
    explicit: Option<CameraInputSurfaceMetrics>,
) -> Option<CameraInputSurfaceMetrics> {
    match (routed, explicit) {
        (None, None) => None,
        (Some(metrics), None) | (None, Some(metrics)) => Some(metrics),
        (Some(mut metrics), Some(explicit)) => {
            if explicit.camera_view_size.is_some() {
                metrics.camera_view_size = explicit.camera_view_size;
            }
            if explicit.input_surface_size.is_some() {
                metrics.input_surface_size = explicit.input_surface_size;
            }
            Some(metrics)
        },
    }
}

fn finalize_camera_input(
    camera: Entity,
    mut input: OrbitCamInput,
    previous: OrbitCamInteractionState,
    blockers: OrbitCamInputBlockers,
    metrics: Option<CameraInputSurfaceMetrics>,
) -> FinalizedInput {
    let mut events = Vec::new();

    if blockers.is_blocked() {
        input.clear();
    } else {
        apply_metric_guard(camera, &mut input, metrics, &mut events);
    }

    let mut state = OrbitCamInteractionState::default();
    let zoom_impulse = input.zoom_impulse_sources();
    let orbit_sources = input.orbit_sources();
    let pan_sources = input.pan_sources();
    let zoom_sources = input.zoom_sources().difference(zoom_impulse);

    push_state_transition(
        camera,
        OrbitCamInteractionKind::Orbit,
        previous.orbit_sources(),
        orbit_sources,
        &mut events,
    );
    push_state_transition(
        camera,
        OrbitCamInteractionKind::Pan,
        previous.pan_sources(),
        pan_sources,
        &mut events,
    );
    push_state_transition(
        camera,
        OrbitCamInteractionKind::Zoom,
        previous.zoom_sources(),
        zoom_sources,
        &mut events,
    );
    push_impulse(
        camera,
        OrbitCamInteractionKind::Zoom,
        zoom_impulse,
        &mut events,
    );

    state.set_sources(OrbitCamInteractionKind::Orbit, orbit_sources);
    state.set_sources(OrbitCamInteractionKind::Pan, pan_sources);
    state.set_sources(OrbitCamInteractionKind::Zoom, zoom_sources);

    FinalizedInput {
        camera,
        input,
        state,
        events,
    }
}

fn apply_metric_guard(
    camera: Entity,
    input: &mut OrbitCamInput,
    metrics: Option<CameraInputSurfaceMetrics>,
    events: &mut Vec<LifecycleEvent>,
) {
    let manual_screen_input = input
        .orbit_sources()
        .union(input.pan_sources())
        .contains(CameraInteractionSources::MANUAL);
    if !manual_screen_input || !input.has_orbit() && !input.has_pan() {
        return;
    }

    let Some(metrics) = metrics else {
        push_missing_metric(camera, CameraInputMetricKind::CameraViewSize, events);
        push_missing_metric(camera, CameraInputMetricKind::InputSurfaceSize, events);
        input.clear_orbit().clear_pan();
        return;
    };

    let mut status = InputMetricStatus::Complete;
    if metrics.camera_view_size.is_none() {
        status = InputMetricStatus::Missing;
        push_missing_metric(camera, CameraInputMetricKind::CameraViewSize, events);
    }
    if metrics.input_surface_size.is_none() {
        status = InputMetricStatus::Missing;
        push_missing_metric(camera, CameraInputMetricKind::InputSurfaceSize, events);
    }
    if matches!(status, InputMetricStatus::Missing) {
        input.clear_orbit().clear_pan();
    }
}

fn push_missing_metric(
    camera: Entity,
    metric: CameraInputMetricKind,
    events: &mut Vec<LifecycleEvent>,
) {
    events.push(LifecycleEvent::MetricsMissing(CameraInputMetricsMissing {
        camera,
        metric,
    }));
}

fn push_state_transition(
    camera: Entity,
    kind: OrbitCamInteractionKind,
    previous: CameraInteractionSources,
    current: CameraInteractionSources,
    events: &mut Vec<LifecycleEvent>,
) {
    match (previous.is_empty(), current.is_empty(), previous == current) {
        (true, false, _) => events.push(LifecycleEvent::Started(OrbitCamInteractionStarted {
            camera,
            kind,
            sources: current,
        })),
        (false, true, _) => events.push(LifecycleEvent::Ended(OrbitCamInteractionEnded {
            camera,
            kind,
            sources: previous,
        })),
        (false, false, false) => {
            events.push(LifecycleEvent::SourcesChanged(
                OrbitCamInteractionSourcesChanged {
                    camera,
                    kind,
                    previous,
                    current,
                },
            ));
        },
        (true, true, _) | (false, false, true) => {},
    }
}

fn push_impulse(
    camera: Entity,
    kind: OrbitCamInteractionKind,
    sources: CameraInteractionSources,
    events: &mut Vec<LifecycleEvent>,
) {
    if sources.is_empty() {
        return;
    }
    events.push(LifecycleEvent::Started(OrbitCamInteractionStarted {
        camera,
        kind,
        sources,
    }));
    events.push(LifecycleEvent::Ended(OrbitCamInteractionEnded {
        camera,
        kind,
        sources,
    }));
}

fn apply_lifecycle_event(world: &mut World, camera: Entity, event: LifecycleEvent) {
    match event {
        LifecycleEvent::Started(event) => {
            world
                .resource_mut::<CameraInputSourceLatches>()
                .acquire_sources(camera, event.sources);
            world.entity_mut(camera).trigger(|_| event);
        },
        LifecycleEvent::Ended(event) => {
            world
                .resource_mut::<CameraInputSourceLatches>()
                .release_sources(camera, event.sources);
            world.entity_mut(camera).trigger(|_| event);
        },
        LifecycleEvent::SourcesChanged(event) => {
            let removed = event.removed_sources();
            let added = event.added_sources();
            {
                let mut latches = world.resource_mut::<CameraInputSourceLatches>();
                latches.release_sources(camera, removed);
                latches.acquire_sources(camera, added);
            }
            world.entity_mut(camera).trigger(|_| event);
        },
        LifecycleEvent::MetricsMissing(event) => {
            world.entity_mut(camera).trigger(|_| event);
        },
    }
}

#[cfg(test)]
mod tests {
    use bevy::camera::RenderTarget;
    use bevy::prelude::*;
    use bevy::window::WindowRef;

    use super::*;
    use crate::OrbitCam;
    use crate::input::CameraInputDisabled;
    use crate::input::CameraInputRoutingConfig;
    use crate::input::OrbitCamInputMode;
    use crate::input::OrbitCamPreset;
    use crate::input::routing::OrbitCamRoutingPlugin;
    use crate::system_sets::LagrangeSystemSetsPlugin;

    #[derive(Resource, Default)]
    struct LifecycleCounts {
        started:         usize,
        ended:           usize,
        sources_changed: usize,
        metrics_missing: usize,
    }

    fn test_app() -> App {
        let mut app = App::new();
        app.add_plugins((
            MinimalPlugins,
            LagrangeSystemSetsPlugin,
            OrbitCamRoutingPlugin,
            OrbitCamInputLifecyclePlugin,
        ));
        app.init_resource::<LifecycleCounts>();
        app
    }

    fn lifecycle_only_app() -> App {
        let mut app = App::new();
        app.add_plugins((
            MinimalPlugins,
            LagrangeSystemSetsPlugin,
            OrbitCamInputLifecyclePlugin,
        ));
        app.init_resource::<LifecycleCounts>()
            .init_resource::<CameraInputSourceLatches>();
        app
    }

    fn spawn_camera(world: &mut World, components: impl Bundle) -> Entity {
        world
            .spawn((
                OrbitCam::default(),
                OrbitCamInput::default(),
                Camera::default(),
                RenderTarget::Window(WindowRef::Primary),
                components,
            ))
            .id()
    }

    fn observe_counts(world: &mut World, camera: Entity) {
        world.entity_mut(camera).observe(
            |_event: On<OrbitCamInteractionStarted>, mut counts: ResMut<LifecycleCounts>| {
                counts.started += 1;
            },
        );
        world.entity_mut(camera).observe(
            |_event: On<OrbitCamInteractionEnded>, mut counts: ResMut<LifecycleCounts>| {
                counts.ended += 1;
            },
        );
        world.entity_mut(camera).observe(
            |_event: On<OrbitCamInteractionSourcesChanged>, mut counts: ResMut<LifecycleCounts>| {
                counts.sources_changed += 1;
            },
        );
        world.entity_mut(camera).observe(
            |_event: On<CameraInputMetricsMissing>, mut counts: ResMut<LifecycleCounts>| {
                counts.metrics_missing += 1;
            },
        );
    }

    type TestResult = Result<(), &'static str>;

    fn update_input(
        app: &mut App,
        camera: Entity,
        apply: impl FnOnce(&mut OrbitCamInput),
    ) -> TestResult {
        let mut input = app
            .world_mut()
            .get_mut::<OrbitCamInput>(camera)
            .ok_or("camera missing OrbitCamInput")?;
        apply(&mut input);
        Ok(())
    }

    fn input(app: &App, camera: Entity) -> Result<OrbitCamInput, &'static str> {
        app.world()
            .get::<OrbitCamInput>(camera)
            .copied()
            .ok_or("camera missing OrbitCamInput")
    }

    fn interaction_state(
        app: &App,
        camera: Entity,
    ) -> Result<OrbitCamInteractionState, &'static str> {
        app.world()
            .get::<OrbitCamInteractionState>(camera)
            .copied()
            .ok_or("camera missing OrbitCamInteractionState")
    }

    #[test]
    fn held_interaction_starts_changes_and_ends() -> TestResult {
        let mut app = test_app();
        let camera = spawn_camera(
            app.world_mut(),
            OrbitCamInputMode::Preset(OrbitCamPreset::SimpleMouse),
        );
        observe_counts(app.world_mut(), camera);
        app.insert_resource(CameraInputRoutingConfig::explicit(camera));

        update_input(&mut app, camera, |input| {
            input.orbit_active_with_sources(CameraInteractionSources::MOUSE);
        })?;
        app.update();
        assert_eq!(app.world().resource::<LifecycleCounts>().started, 1);
        assert_eq!(
            interaction_state(&app, camera)?.orbit_sources(),
            CameraInteractionSources::MOUSE
        );

        update_input(&mut app, camera, |input| {
            input.orbit_active_with_sources(
                CameraInteractionSources::MOUSE.union(CameraInteractionSources::KEYBOARD),
            );
        })?;
        app.update();
        assert_eq!(app.world().resource::<LifecycleCounts>().sources_changed, 1);

        update_input(&mut app, camera, |input| {
            input.clear();
        })?;
        app.update();
        assert_eq!(app.world().resource::<LifecycleCounts>().ended, 1);

        Ok(())
    }

    #[test]
    fn coarse_zoom_impulse_starts_and_ends_same_frame() -> TestResult {
        let mut app = test_app();
        let camera = spawn_camera(
            app.world_mut(),
            OrbitCamInputMode::Preset(OrbitCamPreset::SimpleMouse),
        );
        observe_counts(app.world_mut(), camera);
        app.insert_resource(CameraInputRoutingConfig::explicit(camera));

        update_input(&mut app, camera, |input| {
            input.zoom_coarse_with_sources(1.0, CameraInteractionSources::WHEEL);
        })?;
        app.update();

        let counts = app.world().resource::<LifecycleCounts>();
        assert_eq!(counts.started, 1);
        assert_eq!(counts.ended, 1);
        assert!(interaction_state(&app, camera)?.zoom_sources().is_empty());

        Ok(())
    }

    #[test]
    fn manual_zero_delta_active_state_emits_lifecycle() -> TestResult {
        let mut app = lifecycle_only_app();
        let camera = spawn_camera(
            app.world_mut(),
            (
                OrbitCamInputMode::Manual,
                CameraInputSurfaceMetrics::camera_view_and_input_surface(Vec2::ONE, Vec2::ONE),
            ),
        );
        observe_counts(app.world_mut(), camera);

        update_input(&mut app, camera, |input| {
            input.pan_active_with_sources(CameraInteractionSources::MANUAL);
        })?;
        app.update();

        assert_eq!(app.world().resource::<LifecycleCounts>().started, 1);
        assert_eq!(
            interaction_state(&app, camera)?.pan_sources(),
            CameraInteractionSources::MANUAL
        );

        Ok(())
    }

    #[test]
    fn blocked_camera_ends_existing_interaction() -> TestResult {
        let mut app = test_app();
        let camera = spawn_camera(
            app.world_mut(),
            OrbitCamInputMode::Preset(OrbitCamPreset::SimpleMouse),
        );
        observe_counts(app.world_mut(), camera);
        app.insert_resource(CameraInputRoutingConfig::explicit(camera));

        update_input(&mut app, camera, |input| {
            input.orbit_active_with_sources(CameraInteractionSources::MOUSE);
        })?;
        app.update();
        app.world_mut()
            .entity_mut(camera)
            .insert(CameraInputDisabled);
        update_input(&mut app, camera, |input| {
            input.orbit_active_with_sources(CameraInteractionSources::MOUSE);
        })?;
        app.update();

        assert_eq!(app.world().resource::<LifecycleCounts>().ended, 1);
        assert!(!input(&app, camera)?.has_input());

        Ok(())
    }
}
