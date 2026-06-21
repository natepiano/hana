use std::time::Duration;

use bevy::prelude::*;

use super::CameraInputMetricKind;
use super::CameraInputMetricsMissing;
use super::CameraInputSourceLatches;
use super::CameraInputSurfaceMetrics;
use super::CameraInteractionSources;
use super::ControlSpeed;
use super::OrbitCamInput;
use super::OrbitCamInputBlockers;
use super::OrbitCamInteractionEnded;
use super::OrbitCamInteractionKind;
use super::OrbitCamInteractionSourcesChanged;
use super::OrbitCamInteractionSpeedChanged;
use super::OrbitCamInteractionStarted;
use super::OrbitCamInteractionState;
use super::ResolvedOrbitCamInputRoute;
use super::ZoomDirection;
use super::constants::DEFAULT_REPORTING_DEBOUNCE;
use crate::system_sets::OrbitCamInputPhase;

/// Settle delay that smooths the reported interaction state — both the active
/// source set per kind and the gamepad speed variant.
///
/// Holds a kind's reported sources for this window after the live input goes
/// quiet, bridging the per-frame gaps in bursty input (a trackpad emulating a
/// mouse button as intermittent press/release, smooth-scroll arriving in
/// bursts) so a control panel does not flicker. The same window holds the
/// gamepad speed's return to `Normal`, so the singular variant does not flash
/// when the `rb`/`lb` slow gate lands a frame apart from its stick or trigger.
///
/// Reporting-only: it affects the reported [`OrbitCamInteractionState`] and the
/// [`OrbitCamInteractionStarted`] / [`OrbitCamInteractionEnded`] /
/// [`OrbitCamInteractionSourcesChanged`] / [`OrbitCamInteractionSpeedChanged`]
/// events. Camera motion reads `OrbitCamInput` directly and is never delayed. A
/// newly-engaged source and `Slow` report immediately; only a source drop and
/// the return to `Normal` wait out the window. Insert your own value to
/// override; `Duration::ZERO` disables the delay.
#[derive(Resource, Clone, Copy, Debug, Reflect)]
#[reflect(Resource, Default)]
pub struct OrbitCamReportingDebounce(pub Duration);

impl Default for OrbitCamReportingDebounce {
    fn default() -> Self { Self(DEFAULT_REPORTING_DEBOUNCE) }
}

#[derive(Clone, Copy, Debug)]
enum LifecycleEvent {
    Started(OrbitCamInteractionStarted),
    Ended(OrbitCamInteractionEnded),
    SourcesChanged(OrbitCamInteractionSourcesChanged),
    SpeedChanged(OrbitCamInteractionSpeedChanged),
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
    settle: ReportedInteractionSettle,
    events: Vec<LifecycleEvent>,
}

/// Per-camera settle deadlines (in `Time<Real>` seconds) for the
/// reported-interaction debounce: when each kind's held sources may leave the
/// reported set, and when its gamepad speed may return to `Normal`. `None`
/// means nothing is pending for that axis.
#[derive(Component, Clone, Copy, Debug, Default)]
struct ReportedInteractionSettle {
    orbit: KindSettle,
    pan:   KindSettle,
    zoom:  KindSettle,
}

/// The source and speed settle deadlines tracked per interaction kind.
#[derive(Clone, Copy, Debug, Default)]
struct KindSettle {
    source: Option<f32>,
    speed:  Option<f32>,
}

impl ReportedInteractionSettle {
    const fn source_deadline(self, kind: OrbitCamInteractionKind) -> Option<f32> {
        match kind {
            OrbitCamInteractionKind::Orbit => self.orbit.source,
            OrbitCamInteractionKind::Pan => self.pan.source,
            OrbitCamInteractionKind::Zoom => self.zoom.source,
        }
    }

    const fn speed_deadline(self, kind: OrbitCamInteractionKind) -> Option<f32> {
        match kind {
            OrbitCamInteractionKind::Orbit => self.orbit.speed,
            OrbitCamInteractionKind::Pan => self.pan.speed,
            OrbitCamInteractionKind::Zoom => self.zoom.speed,
        }
    }

    const fn set_source_deadline(&mut self, kind: OrbitCamInteractionKind, at: Option<f32>) {
        match kind {
            OrbitCamInteractionKind::Orbit => self.orbit.source = at,
            OrbitCamInteractionKind::Pan => self.pan.source = at,
            OrbitCamInteractionKind::Zoom => self.zoom.source = at,
        }
    }

    const fn set_speed_deadline(&mut self, kind: OrbitCamInteractionKind, at: Option<f32>) {
        match kind {
            OrbitCamInteractionKind::Orbit => self.orbit.speed = at,
            OrbitCamInteractionKind::Pan => self.pan.speed = at,
            OrbitCamInteractionKind::Zoom => self.zoom.speed = at,
        }
    }
}

/// Transient inputs threaded into finalization for the reported-interaction
/// settle.
#[derive(Clone, Copy)]
struct SettleContext {
    previous: ReportedInteractionSettle,
    now:      f32,
    window:   f32,
}

pub(crate) struct OrbitCamInputLifecyclePlugin;

impl Plugin for OrbitCamInputLifecyclePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<OrbitCamReportingDebounce>()
            .add_systems(
                PreUpdate,
                finalize_orbit_cam_input.in_set(OrbitCamInputPhase::Finalize),
            );
    }
}

fn finalize_orbit_cam_input(world: &mut World) {
    let route = world.get_resource::<ResolvedOrbitCamInputRoute>().cloned();
    let window = world
        .resource::<OrbitCamReportingDebounce>()
        .0
        .as_secs_f32();
    let now = world.resource::<Time<Real>>().elapsed_secs();
    let mut query = world.query::<(
        Entity,
        &OrbitCamInput,
        Option<&OrbitCamInteractionState>,
        Option<&ReportedInteractionSettle>,
        Option<&OrbitCamInputBlockers>,
        Option<&CameraInputSurfaceMetrics>,
    )>();
    let finalized = query
        .iter(world)
        .map(|(camera, input, state, settle, blockers, metrics)| {
            finalize_camera_input(
                camera,
                *input,
                state.copied().unwrap_or_default(),
                SettleContext {
                    previous: settle.copied().unwrap_or_default(),
                    now,
                    window,
                },
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
        world
            .entity_mut(finalized.camera)
            .insert((finalized.state, finalized.settle));
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
    settle_context: SettleContext,
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
    let orbit_sources = input.orbit_sources();
    let pan_sources = input.pan_sources();
    // The coarse-zoom (wheel) source is reported through the same debounce as
    // every other source: it engages for the frame(s) the wheel fires and is
    // held for the reporting window, so a control panel can light the wheel row.
    let zoom_sources = input.zoom_sources();

    let mut settle = ReportedInteractionSettle::default();
    let orbit_reported = debounce_sources(
        &mut settle,
        OrbitCamInteractionKind::Orbit,
        previous.orbit_sources(),
        orbit_sources,
        settle_context,
    );
    let pan_reported = debounce_sources(
        &mut settle,
        OrbitCamInteractionKind::Pan,
        previous.pan_sources(),
        pan_sources,
        settle_context,
    );
    let zoom_reported = debounce_sources(
        &mut settle,
        OrbitCamInteractionKind::Zoom,
        previous.zoom_sources(),
        zoom_sources,
        settle_context,
    );

    push_state_transition(
        camera,
        OrbitCamInteractionKind::Orbit,
        previous.orbit_sources(),
        orbit_reported,
        &mut events,
    );
    push_state_transition(
        camera,
        OrbitCamInteractionKind::Pan,
        previous.pan_sources(),
        pan_reported,
        &mut events,
    );
    push_state_transition(
        camera,
        OrbitCamInteractionKind::Zoom,
        previous.zoom_sources(),
        zoom_reported,
        &mut events,
    );

    state.set_sources(OrbitCamInteractionKind::Orbit, orbit_reported);
    state.set_sources(OrbitCamInteractionKind::Pan, pan_reported);
    state.set_sources(OrbitCamInteractionKind::Zoom, zoom_reported);
    state.set_zoom_direction(reported_zoom_direction(
        &input,
        zoom_reported,
        previous.zoom_direction(),
    ));

    report_speeds(
        camera,
        &input,
        previous,
        settle_context,
        &mut state,
        &mut settle,
        &mut events,
    );

    FinalizedInput {
        camera,
        input,
        state,
        settle,
        events,
    }
}

/// Reports each kind's debounced speed from the reported sources already written
/// to `state`, recording the speed-settle deadline and emitting a
/// [`OrbitCamInteractionSpeedChanged`] event when an engaged kind settles to a
/// new speed.
fn report_speeds(
    camera: Entity,
    input: &OrbitCamInput,
    previous: OrbitCamInteractionState,
    context: SettleContext,
    state: &mut OrbitCamInteractionState,
    settle: &mut ReportedInteractionSettle,
    events: &mut Vec<LifecycleEvent>,
) {
    for (kind, sources, live_speed) in [
        (
            OrbitCamInteractionKind::Orbit,
            state.orbit_sources(),
            input.orbit_speed(),
        ),
        (
            OrbitCamInteractionKind::Pan,
            state.pan_sources(),
            input.pan_speed(),
        ),
        (
            OrbitCamInteractionKind::Zoom,
            state.zoom_sources(),
            input.zoom_speed(),
        ),
    ] {
        let previous_speed = previous.speed(kind);
        let (reported, deadline) = settled_speed(
            previous_speed,
            context.previous.speed_deadline(kind),
            !sources.is_empty(),
            live_speed,
            sources,
            context.now,
            context.window,
        );
        state.set_speed(kind, reported);
        settle.set_speed_deadline(kind, deadline);
        let settled_change = !sources.is_empty() && previous_speed != reported;
        if let Some(speed) = reported.filter(|_| settled_change) {
            events.push(LifecycleEvent::SpeedChanged(
                OrbitCamInteractionSpeedChanged {
                    camera,
                    kind,
                    speed,
                },
            ));
        }
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

/// Computes the debounced reported speed and its pending-settle deadline.
///
/// `Slow` reports immediately. A return to `Normal` — a fresh engage or a chord
/// release — is held back by `window` so the singular variant does not flash for
/// the frame or two a gamepad slow-gate chord straddles. Only the gamepad has a
/// slow gate, so non-gamepad sources report their live speed at once. A `None`
/// report means active-but-unsettled (suppress the singular until it is real).
fn settled_speed(
    previous: Option<ControlSpeed>,
    previous_deadline: Option<f32>,
    active: bool,
    live_speed: ControlSpeed,
    sources: CameraInteractionSources,
    now: f32,
    window: f32,
) -> (Option<ControlSpeed>, Option<f32>) {
    if !active {
        return (None, None);
    }
    if window <= 0.0
        || !sources.intersects(CameraInteractionSources::GAMEPAD)
        || matches!(live_speed, ControlSpeed::Slow)
    {
        return (Some(live_speed), None);
    }
    if matches!(previous, Some(ControlSpeed::Normal)) {
        return (Some(ControlSpeed::Normal), None);
    }
    // Active gamepad at `Normal`, not yet settled: hold the prior report (`Slow`
    // on a chord release, `None` on a fresh engage) until the deadline elapses.
    match previous_deadline {
        None => (previous, Some(now + window)),
        Some(deadline) if now >= deadline => (Some(ControlSpeed::Normal), None),
        Some(_) => (previous, previous_deadline),
    }
}

/// Computes a kind's debounced reported sources and records its source-settle
/// deadline into `settle`. A thin wrapper over [`settled_sources`] that threads
/// the kind's previous deadline in and the new one back out.
fn debounce_sources(
    settle: &mut ReportedInteractionSettle,
    kind: OrbitCamInteractionKind,
    previous: CameraInteractionSources,
    live: CameraInteractionSources,
    context: SettleContext,
) -> CameraInteractionSources {
    let (reported, deadline) = settled_sources(
        previous,
        context.previous.source_deadline(kind),
        live,
        context.now,
        context.window,
    );
    settle.set_source_deadline(kind, deadline);
    reported
}

/// Computes the debounced reported source set and its pending-settle deadline.
///
/// A newly-engaged source reports at once, as does any frame the live set keeps
/// everything it reported. When the live set drops a source, the dropped source
/// stays in the reported set until `window` elapses, so the per-frame gaps in
/// bursty input — a trackpad emulating a mouse button as intermittent
/// press/release, or smooth-scroll arriving in bursts — do not blink the report
/// off. `window <= 0.0` reports the live set verbatim.
fn settled_sources(
    previous: CameraInteractionSources,
    previous_deadline: Option<f32>,
    live: CameraInteractionSources,
    now: f32,
    window: f32,
) -> (CameraInteractionSources, Option<f32>) {
    let dropped = previous.difference(live);
    if window <= 0.0 || dropped.is_empty() {
        return (live, None);
    }
    let held = previous.union(live);
    match previous_deadline {
        None => (held, Some(now + window)),
        Some(deadline) if now >= deadline => (live, None),
        Some(_) => (held, previous_deadline),
    }
}

/// The reported zoom direction from the live zoom sign. Held to the previous
/// direction on a zero-delta frame so it persists through the reporting-debounce
/// window, and cleared when no zoom is reported. Reading the live sign means a
/// reversal (zoom-in to zoom-out) updates at once, without waiting on a settle.
fn reported_zoom_direction(
    input: &OrbitCamInput,
    zoom_reported: CameraInteractionSources,
    previous: Option<ZoomDirection>,
) -> Option<ZoomDirection> {
    if zoom_reported.is_empty() {
        return None;
    }
    let amount = input.zoom_coarse().amount() + input.zoom_smooth().amount();
    if amount > 0.0 {
        Some(ZoomDirection::In)
    } else if amount < 0.0 {
        Some(ZoomDirection::Out)
    } else {
        previous
    }
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
        LifecycleEvent::SpeedChanged(event) => {
            world.entity_mut(camera).trigger(|_| event);
        },
        LifecycleEvent::MetricsMissing(event) => {
            world.entity_mut(camera).trigger(|_| event);
        },
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

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
        // The integration tests exercise the raw start/change/end lifecycle;
        // disable the reporting debounce so a source drop ends in the same frame.
        app.insert_resource(OrbitCamReportingDebounce(Duration::ZERO));
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
        app.insert_resource(OrbitCamReportingDebounce(Duration::ZERO));
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
    fn coarse_zoom_registers_then_ends_when_input_clears() -> TestResult {
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
        assert_eq!(app.world().resource::<LifecycleCounts>().started, 1);
        assert_eq!(
            interaction_state(&app, camera)?.zoom_sources(),
            CameraInteractionSources::WHEEL
        );

        // The wheel is a one-frame impulse: once its input clears, the zoom
        // interaction ends — immediately here, since `test_app` disables the
        // reporting debounce.
        update_input(&mut app, camera, |input| {
            input.clear();
        })?;
        app.update();
        assert_eq!(app.world().resource::<LifecycleCounts>().ended, 1);
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

    const WINDOW: f32 = 0.5;

    #[test]
    fn fresh_gamepad_normal_stays_pending_until_window_then_settles() {
        let gamepad = CameraInteractionSources::GAMEPAD;
        // Fresh engage at Normal reports pending (None) and arms the deadline.
        let (reported, deadline) =
            settled_speed(None, None, true, ControlSpeed::Normal, gamepad, 1.0, WINDOW);
        assert_eq!(reported, None);
        assert_eq!(deadline, Some(1.5));
        // Within the window it stays pending.
        let (reported, _) = settled_speed(
            None,
            Some(1.5),
            true,
            ControlSpeed::Normal,
            gamepad,
            1.25,
            WINDOW,
        );
        assert_eq!(reported, None);
        // Once the deadline passes it settles to Normal.
        let (reported, deadline) = settled_speed(
            None,
            Some(1.5),
            true,
            ControlSpeed::Normal,
            gamepad,
            1.5,
            WINDOW,
        );
        assert_eq!(reported, Some(ControlSpeed::Normal));
        assert_eq!(deadline, None);
    }

    #[test]
    fn gamepad_slow_reports_immediately() {
        let gamepad = CameraInteractionSources::GAMEPAD;
        let (reported, deadline) =
            settled_speed(None, None, true, ControlSpeed::Slow, gamepad, 1.0, WINDOW);
        assert_eq!(reported, Some(ControlSpeed::Slow));
        assert_eq!(deadline, None);
    }

    #[test]
    fn chord_release_holds_slow_until_window_then_settles() {
        let gamepad = CameraInteractionSources::GAMEPAD;
        // Live drops to Normal on release; the prior Slow is held and armed.
        let (reported, deadline) = settled_speed(
            Some(ControlSpeed::Slow),
            None,
            true,
            ControlSpeed::Normal,
            gamepad,
            2.0,
            WINDOW,
        );
        assert_eq!(reported, Some(ControlSpeed::Slow));
        assert_eq!(deadline, Some(2.5));
        // After the deadline it settles to Normal.
        let (reported, _) = settled_speed(
            Some(ControlSpeed::Slow),
            Some(2.5),
            true,
            ControlSpeed::Normal,
            gamepad,
            2.5,
            WINDOW,
        );
        assert_eq!(reported, Some(ControlSpeed::Normal));
    }

    #[test]
    fn non_gamepad_normal_reports_immediately() {
        let mouse = CameraInteractionSources::MOUSE;
        let (reported, deadline) =
            settled_speed(None, None, true, ControlSpeed::Normal, mouse, 1.0, WINDOW);
        assert_eq!(reported, Some(ControlSpeed::Normal));
        assert_eq!(deadline, None);
    }

    #[test]
    fn zero_window_disables_the_settle_delay() {
        let gamepad = CameraInteractionSources::GAMEPAD;
        let (reported, deadline) =
            settled_speed(None, None, true, ControlSpeed::Normal, gamepad, 1.0, 0.0);
        assert_eq!(reported, Some(ControlSpeed::Normal));
        assert_eq!(deadline, None);
    }

    #[test]
    fn inactive_kind_clears_report_and_deadline() {
        let gamepad = CameraInteractionSources::GAMEPAD;
        let (reported, deadline) = settled_speed(
            Some(ControlSpeed::Slow),
            Some(5.0),
            false,
            ControlSpeed::Normal,
            gamepad,
            1.0,
            WINDOW,
        );
        assert_eq!(reported, None);
        assert_eq!(deadline, None);
    }

    #[test]
    fn fresh_source_reports_immediately() {
        let (reported, deadline) = settled_sources(
            CameraInteractionSources::NONE,
            None,
            CameraInteractionSources::MOUSE,
            1.0,
            WINDOW,
        );
        assert_eq!(reported, CameraInteractionSources::MOUSE);
        assert_eq!(deadline, None);
    }

    #[test]
    fn gaining_a_source_reports_immediately() {
        let both = CameraInteractionSources::MOUSE.union(CameraInteractionSources::KEYBOARD);
        let (reported, deadline) =
            settled_sources(CameraInteractionSources::MOUSE, None, both, 1.0, WINDOW);
        assert_eq!(reported, both);
        assert_eq!(deadline, None);
    }

    #[test]
    fn dropped_source_holds_until_window_then_clears() {
        let mouse = CameraInteractionSources::MOUSE;
        // Live drops to nothing: the source is held and the deadline armed.
        let (reported, deadline) =
            settled_sources(mouse, None, CameraInteractionSources::NONE, 2.0, WINDOW);
        assert_eq!(reported, mouse);
        assert_eq!(deadline, Some(2.5));
        // Within the window it stays held.
        let (reported, _) = settled_sources(
            mouse,
            Some(2.5),
            CameraInteractionSources::NONE,
            2.25,
            WINDOW,
        );
        assert_eq!(reported, mouse);
        // Once the deadline passes the source clears.
        let (reported, deadline) = settled_sources(
            mouse,
            Some(2.5),
            CameraInteractionSources::NONE,
            2.5,
            WINDOW,
        );
        assert!(reported.is_empty());
        assert_eq!(deadline, None);
    }

    #[test]
    fn re_engaging_during_hold_clears_the_deadline() {
        let mouse = CameraInteractionSources::MOUSE;
        let (reported, deadline) = settled_sources(mouse, Some(2.5), mouse, 2.25, WINDOW);
        assert_eq!(reported, mouse);
        assert_eq!(deadline, None);
    }

    #[test]
    fn partial_drop_holds_the_union_until_the_window() {
        let both = CameraInteractionSources::MOUSE.union(CameraInteractionSources::KEYBOARD);
        // KEYBOARD drops while MOUSE stays live: the union is held.
        let (reported, deadline) =
            settled_sources(both, None, CameraInteractionSources::MOUSE, 1.0, WINDOW);
        assert_eq!(reported, both);
        assert_eq!(deadline, Some(1.5));
    }

    #[test]
    fn zero_window_reports_live_sources_immediately() {
        let (reported, deadline) = settled_sources(
            CameraInteractionSources::MOUSE,
            None,
            CameraInteractionSources::NONE,
            1.0,
            0.0,
        );
        assert!(reported.is_empty());
        assert_eq!(deadline, None);
    }
}
