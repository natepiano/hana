//! The hardware-free rule suite.
//!
//! Every case here drives the kernel through `hana_rigging_scripted::ScriptedReporter`, which
//! replays a written list of whole-set scans. No test touches hardware, and no test mocks anything
//! but the scan list and the driver: a rule that cannot be reached this way has put input or output
//! inside the kernel, which is the defect this suite exists to catch.

use std::error::Error;
use std::num::NonZeroU32;
use std::time::Duration;

use bevy::MinimalPlugins;
use bevy::app::App;
use bevy::ecs::reflect::AppTypeRegistry;
use bevy::ecs::reflect::ReflectComponent;
use bevy::prelude::Component;
use bevy::prelude::On;
use bevy::prelude::Reflect;
use bevy::prelude::ResMut;
use bevy::prelude::Resource;
use bevy::prelude::World;
use bevy::time::Real;
use bevy::time::Time;
use hana_rigging::prelude::*;
use hana_rigging_scripted::ScriptedDevice;
use hana_rigging_scripted::ScriptedReporter;
use hana_rigging_scripted::ScriptedScan;
use hana_rigging_scripted::advance_reporter;
use hana_rigging_scripted::advance_until_accepted;
use hana_rigging_scripted::advance_until_running;
use hana_rigging_scripted::reported_key;
use hana_rigging_scripted::scan;

/// Identity space every scripted panel in this suite is named in.
const PANEL_SCHEME: &str = "usb-serial";

/// Slot the scripted driver reports the endpoint is already sitting in.
const CAPTURED_SLOT: u32 = 7;

/// Slot every scripted role asks its driver to move the endpoint to.
const REQUESTED_SLOT: u32 = 1;

/// Transport text the scripted driver reports when a case asks it to fail an apply.
const DRIVER_FAILURE_DETAIL: &str = "the scripted driver was asked to refuse this apply";

/// Transport text a scripted reporter reports when a case asks it to fail an enumeration.
const DISCOVERY_FAILURE_DETAIL: &str = "the scripted reporter was asked to fail this scan";

/// Placement the scripted role asks its driver for.
#[derive(Component, Reflect)]
#[reflect(Component)]
struct PanelPlacement {
    slot: u32,
}

/// What the scripted driver was asked to do, in dispatch order.
///
/// Written through the `World` the driver is handed rather than kept in the driver value, because
/// the driver lives inside the kernel's registry and the test never sees it again after
/// registration.
#[derive(Default, Resource)]
struct DriverCalls {
    /// One entry per `start_apply`, holding the identifier the kernel minted for it.
    started:            Vec<AttemptId>,
    /// How many more polls answer with a failure before the driver starts converging.
    ///
    /// Zero for every case that is not about retry, so a test opts into failure rather than
    /// spelling out a second driver type whose only difference is its first answer.
    failures_remaining: usize,
}

/// Driver that records every dispatch and reports the apply converged on the first poll.
///
/// Its readback answers `CaptureOutcome::Read` rather than `CaptureOutcome::NotReadable` because a
/// role whose endpoint exposes nothing never establishes a `LastKnownGoodConfiguration::Known`, and
/// `RecoveryPolicy::ReapplyOnReturn` has no saved value to owe back on a departure — so an
/// unreadable scripted endpoint would silently disable every recovery case in this suite.
struct RecordingDriver;

impl EndpointDriver for RecordingDriver {
    type Configuration = PanelPlacement;

    fn capture(
        &mut self,
        _: &mut World,
        _: &DeviceEndpoint,
    ) -> CaptureOutcome<Self::Configuration> {
        CaptureOutcome::Read(PanelPlacement {
            slot: CAPTURED_SLOT,
        })
    }

    fn start_apply(
        &mut self,
        world: &mut World,
        _: &DeviceEndpoint,
        _: &Self::Configuration,
        attempt: AttemptId,
        _: ApplyPermit,
    ) {
        world.resource_mut::<DriverCalls>().started.push(attempt);
    }

    fn poll(&mut self, world: &mut World, _: AttemptId) -> AttemptProgress {
        let mut driver_calls = world.resource_mut::<DriverCalls>();
        if driver_calls.failures_remaining == 0 {
            return AttemptProgress::Finished(AttemptOutcome::Succeeded);
        }
        driver_calls.failures_remaining -= 1;

        AttemptProgress::Finished(AttemptOutcome::Failed(DeviceAccessError::Transport {
            detail: DRIVER_FAILURE_DETAIL.to_owned(),
        }))
    }
}

/// Every event this suite observes, in arrival order per axis.
#[derive(Default, Resource)]
struct ObservedEvents {
    presences:          Vec<Presence>,
    claims:             Vec<Claim>,
    arrivals:           Vec<DeviceKey>,
    departures:         Vec<(DeviceKey, DeviceDeparture)>,
    role_state:         Vec<RoleState>,
    schemes:            Vec<SchemeName>,
    connections:        Vec<(DeviceKey, ConfiguredDeviceConnection)>,
    attempt_outcomes:   Vec<AttemptOutcome>,
    startup:            Vec<StartupDiscoveryState>,
    reporter_progress:  Vec<(DiscoveryBatchId, ReporterId, DiscoveryProgress)>,
    hardware_progress:  Vec<ObservedDiscoveryCounts>,
    discovery_finished: Vec<(DiscoveryBatchId, ReporterId, CompletedDiscoveryOutcome)>,
}

/// The aggregate counts one `HardwareDiscoveryProgress` carried.
///
/// Kept as a named record rather than a tuple because the four counts are all `usize` and a test
/// comparing them positionally would pass with any two of them swapped.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct ObservedDiscoveryCounts {
    batch:     DiscoveryBatchId,
    completed: usize,
    total:     usize,
    running:   usize,
    queued:    usize,
}

fn observe_presence(event: On<PresenceChanged>, mut observed: ResMut<ObservedEvents>) {
    observed.presences.push(event.presence);
}

fn observe_claim(event: On<ClaimChanged>, mut observed: ResMut<ObservedEvents>) {
    observed.claims.push(event.claim.clone());
}

fn observe_arrival(event: On<DeviceArrived>, mut observed: ResMut<ObservedEvents>) {
    observed.arrivals.push(event.key.clone());
}

fn observe_departure(event: On<DeviceDeparted>, mut observed: ResMut<ObservedEvents>) {
    observed
        .departures
        .push((event.key.clone(), event.departure));
}

fn observe_role_state(event: On<RoleStateChanged>, mut observed: ResMut<ObservedEvents>) {
    observed.role_state.push(event.state);
}

fn observe_scheme(event: On<UnregisteredSchemeReported>, mut observed: ResMut<ObservedEvents>) {
    observed.schemes.push(event.scheme.clone());
}

fn observe_connection(
    event: On<ConfiguredDeviceConnectionChanged>,
    mut observed: ResMut<ObservedEvents>,
) {
    observed
        .connections
        .push((event.key.clone(), event.connection));
}

fn observe_attempt_finished(event: On<AttemptFinished>, mut observed: ResMut<ObservedEvents>) {
    observed.attempt_outcomes.push(event.outcome.clone());
}

fn observe_startup(event: On<StartupDiscoveryChanged>, mut observed: ResMut<ObservedEvents>) {
    observed.startup.push(event.state.clone());
}

fn observe_reporter_progress(
    event: On<DiscoveryProgressChanged>,
    mut observed: ResMut<ObservedEvents>,
) {
    observed
        .reporter_progress
        .push((event.batch, event.reporter, event.progress.clone()));
}

fn observe_hardware_progress(
    event: On<HardwareDiscoveryProgress>,
    mut observed: ResMut<ObservedEvents>,
) {
    observed.hardware_progress.push(ObservedDiscoveryCounts {
        batch:     event.batch,
        completed: event.completed,
        total:     event.total,
        running:   event.running,
        queued:    event.queued,
    });
}

fn observe_discovery_finished(event: On<DiscoveryFinished>, mut observed: ResMut<ObservedEvents>) {
    observed
        .discovery_finished
        .push((event.batch, event.reporter, event.outcome.clone()));
}

/// Build an app with the kernel, the observers, and one scripted reporter that establishes absence.
fn scripted_app(scans: Vec<ScriptedScan>) -> Result<(App, ReporterId), Box<dyn Error>> {
    let mut app = observing_app()?;
    let reporter = app.add_device_reporter(
        ScriptedReporter::new(scans),
        ReporterRegistration::optional(
            DiscoveryCadence::OnDemand,
            ReporterActivation::Enabled,
            panel_coverage()?,
        ),
    );

    Ok((app, reporter))
}

/// Build the same app around a reporter startup is not allowed to proceed without.
///
/// A separate builder rather than a flag on `scripted_app`: `ReporterRegistration::required`
/// carries no activation argument, because a reporter startup waits for cannot also be one the
/// application leaves disabled.
fn required_scripted_app(scans: Vec<ScriptedScan>) -> Result<(App, ReporterId), Box<dyn Error>> {
    let mut app = observing_app()?;
    let reporter = app.add_device_reporter(
        ScriptedReporter::new(scans),
        ReporterRegistration::required(DiscoveryCadence::OnDemand, panel_coverage()?),
    );

    Ok((app, reporter))
}

/// The kernel, the scripted scheme, and every observer this suite reads events through.
fn observing_app() -> Result<App, Box<dyn Error>> {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .add_plugins(RiggingPlugin)
        .init_resource::<DriverCalls>()
        .init_resource::<ObservedEvents>()
        .register_device_scheme(SchemeName::new(PANEL_SCHEME)?)
        .add_observer(observe_presence)
        .add_observer(observe_claim)
        .add_observer(observe_arrival)
        .add_observer(observe_departure)
        .add_observer(observe_role_state)
        .add_observer(observe_scheme)
        .add_observer(observe_connection)
        .add_observer(observe_attempt_finished)
        .add_observer(observe_startup)
        .add_observer(observe_reporter_progress)
        .add_observer(observe_hardware_progress)
        .add_observer(observe_discovery_finished);

    Ok(app)
}

/// Coverage that makes a scripted reporter's omission of a panel key mean the unit is gone.
///
/// Authoritative rather than `ReporterCoverage::MatchingEvidenceOnly`: an evidence-only reporter's
/// omission of a key proves nothing, so it can produce no departure, no `DeviceDeparted`, and no
/// absent-then-present reacquisition — the three things most of this suite is about.
fn panel_coverage() -> Result<ReporterCoverage, Box<dyn Error>> {
    Ok(ReporterCoverage::EstablishesAbsence(
        AuthoritativeReporterCoverage::one(CoveredDeviceIdentitySpace::ReportedScheme {
            kind:   DeviceKind::HidPanel,
            scheme: SchemeName::new(PANEL_SCHEME)?,
        }),
    ))
}

/// Register one role against a scripted panel key, with an authored or defaulted apply deadline.
fn register_role(
    app: &mut App,
    role: &RoleKey,
    device: DeviceKey,
    recovery: RecoveryPolicy,
    retry: RetryOn,
    apply_deadline: ApplyDeadline,
) -> Result<(), Box<dyn Error>> {
    let driver = app.add_endpoint_driver(RecordingDriver);
    app.world_mut()
        .resource_mut::<Bindings>()
        .register(Binding {
            role: role.clone(),
            endpoint: DeviceEndpoint {
                device,
                id: EndpointId::Whole,
            },
            driver,
            recovery,
            retry,
            on_abort: OnAbort::default(),
            on_loss: OnSessionLoss::default(),
            state: RoleState::default(),
            requested: RequestedConfiguration::new(PanelPlacement {
                slot: REQUESTED_SLOT,
            }),
            last_known_good: LastKnownGoodConfiguration::default(),
            apply_deadline,
        })?;

    Ok(())
}

fn panel_key(value: &str) -> Result<DeviceKey, Box<dyn Error>> {
    Ok(reported_key(DeviceKind::HidPanel, PANEL_SCHEME, value)?)
}

/// A record naming a scheme nobody registered must reach a consumer rather than vanishing.
///
/// Ingest rejects the record, so it produces no device, no mirrored component, and therefore no
/// event under the derived-from-mirrored-components rule. Without this event a reporter author
/// debugging a typo'd scheme has nothing at all to look at.
#[test]
fn an_unregistered_scheme_reaches_a_consumer() -> Result<(), Box<dyn Error>> {
    let mistyped = reported_key(DeviceKind::HidPanel, "usb-seral", "CL15")?;
    let (mut app, reporter) = scripted_app(vec![scan![ScriptedDevice::present(mistyped)]])?;

    advance_reporter(&mut app, reporter)?;

    let scheme = SchemeName::new("usb-seral")?;
    assert!(
        app.world()
            .resource::<Devices>()
            .unregistered_schemes()
            .contains(&scheme)
    );
    assert_eq!(
        app.world().resource::<ObservedEvents>().schemes,
        vec![scheme]
    );

    Ok(())
}

/// A device nobody else can use must not be seized, however much the role wants it.
///
/// `RecoveryPolicy::ReapplyOnReturn` is the variant that acts on its own, so it is the one that
/// would take a contended unit if the claim were not consulted.
#[test]
fn a_contended_device_does_not_reacquire() -> Result<(), Box<dyn Error>> {
    let key = panel_key("CL15")?;
    let (mut app, reporter) = scripted_app(vec![scan![
        ScriptedDevice::present(key.clone()).with_claim(Claim::Contended {
            holder: ClaimHolder::Unidentified,
        })
    ]])?;
    let role = RoleKey::new("panel")?;
    register_role(
        &mut app,
        &role,
        key,
        RecoveryPolicy::ReapplyOnReturn,
        RetryOn::NewRevision,
        ApplyDeadline::ProcessDefault,
    )?;

    advance_reporter(&mut app, reporter)?;
    app.update();

    assert_eq!(
        app.world().resource::<Bindings>().binding(&role)?.state,
        RoleState::Waiting
    );
    assert!(app.world().resource::<Attempts>().is_empty());
    assert!(app.world().resource::<DriverCalls>().started.is_empty());

    Ok(())
}

/// A unit that leaves and returns must be applied again, and the departure must name it.
#[test]
fn an_absent_then_present_cycle_reacquires() -> Result<(), Box<dyn Error>> {
    let key = panel_key("CL15")?;
    let (mut app, reporter) = scripted_app(vec![
        scan![ScriptedDevice::present(key.clone())],
        scan![],
        scan![ScriptedDevice::present(key.clone())],
    ])?;
    let role = RoleKey::new("panel")?;
    register_role(
        &mut app,
        &role,
        key.clone(),
        RecoveryPolicy::ReapplyOnReturn,
        RetryOn::NewRevision,
        ApplyDeadline::ProcessDefault,
    )?;

    advance_reporter(&mut app, reporter)?;
    app.update();
    let applies_after_arrival = app.world().resource::<DriverCalls>().started.len();

    advance_reporter(&mut app, reporter)?;
    app.update();

    advance_reporter(&mut app, reporter)?;
    for _ in 0..16 {
        if app.world().resource::<DriverCalls>().started.len() > applies_after_arrival {
            break;
        }
        app.update();
    }

    let observed = app.world().resource::<ObservedEvents>();
    assert_eq!(applies_after_arrival, 1);
    // Twice: a `DeviceDeparture::KeyLeftTheSet` despawns the device entity, so the unit's return
    // spawns a new one and arrives again. The durable key is what ties the two together.
    assert_eq!(observed.arrivals, vec![key.clone(), key.clone()]);
    assert_eq!(
        observed
            .departures
            .iter()
            .map(|(departed_key, _)| departed_key.clone())
            .collect::<Vec<_>>(),
        vec![key]
    );
    assert_eq!(app.world().resource::<DriverCalls>().started.len(), 2);

    Ok(())
}

/// A frame in which nothing moved must emit nothing at all.
///
/// The mirrors are written only when a value differs, so a settled frame leaves every mirror
/// untouched and the event stage has nothing to derive. A mirror that rewrote an equal value would
/// make every once-per-change consumer fire forever, which is what this case guards.
#[test]
fn a_settled_frame_emits_nothing() -> Result<(), Box<dyn Error>> {
    let key = panel_key("CL15")?;
    let (mut app, reporter) = scripted_app(vec![scan![ScriptedDevice::present(key)]])?;

    advance_reporter(&mut app, reporter)?;
    app.update();
    let settled_from = observed_counts(&app);
    app.update();
    app.update();

    assert_eq!(observed_counts(&app), settled_from);

    Ok(())
}

/// Every mirrored axis must fire exactly once per change and never on a repeat of the same value.
#[test]
fn a_mirrored_axis_fires_once_per_change() -> Result<(), Box<dyn Error>> {
    let key = panel_key("CL15")?;
    let (mut app, reporter) = scripted_app(vec![
        scan![ScriptedDevice::present(key.clone()).with_claim(Claim::Free)],
        scan![ScriptedDevice::present(key.clone()).with_claim(Claim::Free)],
        scan![ScriptedDevice::present(key).with_claim(Claim::Held)],
    ])?;

    for _ in 0..3 {
        advance_reporter(&mut app, reporter)?;
        app.update();
    }

    let observed = app.world().resource::<ObservedEvents>();
    assert_eq!(observed.presences, vec![Presence::Present]);
    assert_eq!(observed.claims, vec![Claim::Free, Claim::Held]);

    Ok(())
}

/// An authored per-binding deadline must reach the attempt.
///
/// One process drives endpoints with genuinely different costs, so a single process-wide bound
/// either abandons the slow one or lets the fast one hang.
#[test]
fn an_authored_apply_deadline_is_stamped_on_the_attempt() -> Result<(), Box<dyn Error>> {
    let authored = Duration::from_secs(5);
    assert_eq!(
        rounded_deadline_gap(ApplyDeadline::Authored(authored))?,
        authored
    );

    Ok(())
}

/// A binding that authors no deadline of its own must be stamped with the process-wide one.
///
/// `ApplyDeadline::ProcessDefault` is the variant almost every binding uses, so a fallback that
/// silently stamped nothing — or stamped a hard-coded constant of its own — would leave the
/// kernel's one configurable bound unreachable for every ordinary role.
#[test]
fn an_unauthored_apply_deadline_falls_back_to_the_process_default() -> Result<(), Box<dyn Error>> {
    assert_eq!(
        rounded_deadline_gap(ApplyDeadline::ProcessDefault)?,
        RiggingLimits::default().apply_deadline
    );

    Ok(())
}

/// A key duplicated inside one scan must be suppressed without suppressing the rest of the scan.
///
/// The whole point of the rule is that it is per key: a reporter that repeats one weakly-identified
/// panel is still telling the truth about every other unit it named, and a pass that dropped them
/// all would turn one ambiguous webcam into a total discovery outage.
#[test]
fn duplicate_key_suppression_is_per_key() -> Result<(), Box<dyn Error>> {
    let duplicated = panel_key("CL15")?;
    let single = panel_key("CL16")?;
    let (mut app, reporter) = scripted_app(vec![scan![
        ScriptedDevice::present(duplicated.clone()),
        ScriptedDevice::present(duplicated.clone()),
        ScriptedDevice::present(single.clone()),
    ]])?;
    let duplicated_role = RoleKey::new("duplicated-panel")?;
    let single_role = RoleKey::new("single-panel")?;
    register_role(
        &mut app,
        &duplicated_role,
        duplicated.clone(),
        RecoveryPolicy::default(),
        RetryOn::NewRevision,
        ApplyDeadline::ProcessDefault,
    )?;
    register_role(
        &mut app,
        &single_role,
        single.clone(),
        RecoveryPolicy::default(),
        RetryOn::NewRevision,
        ApplyDeadline::ProcessDefault,
    )?;

    advance_reporter(&mut app, reporter)?;

    let devices = app.world().resource::<Devices>();
    assert!(devices.duplicate_keys().contains(&duplicated));
    assert!(!devices.duplicate_keys().contains(&single));
    assert_eq!(
        app.world()
            .resource::<Bindings>()
            .binding(&duplicated_role)?
            .state,
        RoleState::Waiting
    );
    // One dispatch, and the suppressed role is the one still waiting, so it was the unsuppressed
    // key that reached a driver.
    assert_eq!(app.world().resource::<DriverCalls>().started.len(), 1);

    Ok(())
}

/// Progress must stay silent until a run outlasts `DiscoveryLimits::progress_after`, and the
/// per-reporter and aggregate views of the same run must never disagree.
///
/// The delay is what keeps an interface from flashing a progress indicator for every scan that
/// finishes in a millisecond. The agreement matters because both events are derived from one
/// recorded transition: a second derivation would let the reporter row and the summary bar report
/// different batches.
#[test]
fn discovery_progress_waits_for_its_delay_then_agrees_across_both_views()
-> Result<(), Box<dyn Error>> {
    let key = panel_key("CL15")?;
    let scripted_progress = DiscoveryProgress::Measured {
        completed: 1,
        total:     NonZeroU32::new(4).ok_or("four is not zero")?,
    };
    // Held on the I/O pool rather than run inline: an immediate scan finishes inside the admission
    // call that started it, so the kernel never retains a running activity to progress at all.
    let (scripted_reporter, gate) = ScriptedReporter::gated(
        vec![scan![ScriptedDevice::present(key)]],
        scripted_progress.clone(),
    );
    let mut app = observing_app()?;
    // Longer than the case can run, so the silent half is decided by the delay and not by timing.
    app.world_mut()
        .resource_mut::<DiscoveryLimits>()
        .set_progress_after(Duration::from_hours(1));
    let reporter = app.add_device_reporter(
        scripted_reporter,
        ReporterRegistration::optional(
            DiscoveryCadence::OnDemand,
            ReporterActivation::Enabled,
            panel_coverage()?,
        ),
    );

    advance_until_running(&mut app, reporter)?;

    {
        let observed = app.world().resource::<ObservedEvents>();
        assert!(observed.reporter_progress.is_empty());
        assert!(observed.hardware_progress.is_empty());
    }

    app.world_mut()
        .resource_mut::<DiscoveryLimits>()
        .set_progress_after(Duration::ZERO);
    app.update();

    {
        let observed = app.world().resource::<ObservedEvents>();
        assert!(!observed.reporter_progress.is_empty());
        assert_eq!(
            observed.reporter_progress.len(),
            observed.hardware_progress.len()
        );
        for ((batch, progressed_by, progress), counts) in observed
            .reporter_progress
            .iter()
            .zip(&observed.hardware_progress)
        {
            assert_eq!(*progressed_by, reporter);
            assert_eq!(*progress, scripted_progress);
            assert_eq!(counts.batch, *batch);
            assert_eq!(counts.total, 1);
            assert_eq!(counts.completed + counts.running + counts.queued, 1);
        }
    }

    gate.release();
    advance_until_accepted(&mut app, reporter)?;

    Ok(())
}

/// A required reporter's failure must close startup, and its later success must open it.
///
/// Startup readiness is the one discovery fact the rest of an application gates on, so a failure
/// that left it in `StartupDiscoveryState::Discovering` would read as a slow probe forever and no
/// consumer would ever learn the enumeration is broken.
#[test]
fn a_required_discovery_failure_blocks_startup_until_a_later_success() -> Result<(), Box<dyn Error>>
{
    let key = panel_key("CL15")?;
    let (mut app, reporter) = required_scripted_app(vec![
        ScriptedScan::Failed(DeviceAccessError::Transport {
            detail: DISCOVERY_FAILURE_DETAIL.to_owned(),
        }),
        scan![ScriptedDevice::present(key)],
    ])?;

    advance_reporter(&mut app, reporter)?;

    let blocked = app
        .world()
        .resource::<ObservedEvents>()
        .startup
        .last()
        .cloned()
        .ok_or("a required reporter's failure announced no startup state")?;
    assert!(matches!(
        blocked,
        StartupDiscoveryState::BlockedByFailure {
            reporter: blocked_by,
            ..
        } if blocked_by == reporter
    ));

    assert!(matches!(
        app.world()
            .resource::<ObservedEvents>()
            .discovery_finished
            .last(),
        Some((_, finished_by, CompletedDiscoveryOutcome::Failed { .. }))
            if *finished_by == reporter
    ));

    advance_reporter(&mut app, reporter)?;

    let observed = app.world().resource::<ObservedEvents>();
    assert_eq!(observed.startup.last(), Some(&StartupDiscoveryState::Ready));
    assert!(matches!(
        observed.discovery_finished.last(),
        Some((_, _, CompletedDiscoveryOutcome::Succeeded { .. }))
    ));

    Ok(())
}

/// An authored device the application marked offline must still report its connectivity, and no
/// driver may be touched for it.
///
/// The two halves are the whole point of `ConfiguredDeviceMode::Offline`: a user interface has to
/// be able to show a configured-but-disabled fixture as plugged in, and the kernel has to keep its
/// promise that nothing drives it while it is offline.
#[test]
fn an_offline_authored_device_reports_connection_without_touching_its_driver()
-> Result<(), Box<dyn Error>> {
    let key = panel_key("CL15")?;
    let (mut app, reporter) = scripted_app(vec![scan![ScriptedDevice::present(key.clone())]])?;
    app.world_mut()
        .resource_mut::<HardwareInventory>()
        .configure(ConfiguredDevice {
            key:  key.clone(),
            mode: ConfiguredDeviceMode::Offline,
        });
    let role = RoleKey::new("panel")?;
    register_role(
        &mut app,
        &role,
        key.clone(),
        RecoveryPolicy::default(),
        RetryOn::NewRevision,
        ApplyDeadline::ProcessDefault,
    )?;

    advance_reporter(&mut app, reporter)?;
    app.update();

    assert_eq!(
        app.world()
            .resource::<HardwareInventory>()
            .connection(&key)?,
        ConfiguredDeviceConnection::Present
    );
    let observed = app.world().resource::<ObservedEvents>();
    assert!(
        observed
            .connections
            .contains(&(key, ConfiguredDeviceConnection::Present))
    );
    assert!(observed.attempt_outcomes.is_empty());
    assert!(app.world().resource::<DriverCalls>().started.is_empty());

    Ok(())
}

/// A key no reporter names any more must depart as `DeviceDeparture::KeyLeftTheSet`, and the event
/// must still name it after its entity is gone.
#[test]
fn a_key_that_left_the_set_departs_after_its_entity_is_despawned() -> Result<(), Box<dyn Error>> {
    let key = panel_key("CL15")?;
    let departure = departure_after(&key, scan![])?;

    assert_eq!(departure.key, key);
    assert_eq!(departure.departure, DeviceDeparture::KeyLeftTheSet);
    assert_eq!(departure.retained_devices, 0);

    Ok(())
}

/// A key a reporter still names while reporting the unit gone must depart as
/// `DeviceDeparture::RetainedButNotPresent` with its device entity alive, and the two departure
/// kinds must be told apart from the event payload alone.
///
/// A consumer of the despawning kind has no entity left to inspect, so if the payload did not carry
/// the distinction it could not be recovered from anywhere at all.
#[test]
fn a_retained_but_absent_key_departs_with_its_entity_alive() -> Result<(), Box<dyn Error>> {
    let key = panel_key("CL15")?;
    let retained = departure_after(&key, scan![ScriptedDevice::absent(key.clone())])?;
    let left_the_set = departure_after(&key, scan![])?;

    assert_eq!(retained.key, key);
    assert_eq!(retained.departure, DeviceDeparture::RetainedButNotPresent);
    assert_eq!(retained.retained_devices, 1);
    assert_ne!(retained.departure, left_the_set.departure);

    Ok(())
}

/// What one scripted departure reported, and how much device state outlived it.
struct ObservedDeparture {
    key:              DeviceKey,
    departure:        DeviceDeparture,
    retained_devices: usize,
}

/// Introduce `key`, then replace the scan with `second` and report the one departure that follows.
fn departure_after(
    key: &DeviceKey,
    second: ScriptedScan,
) -> Result<ObservedDeparture, Box<dyn Error>> {
    let (mut app, reporter) =
        scripted_app(vec![scan![ScriptedDevice::present(key.clone())], second])?;

    advance_reporter(&mut app, reporter)?;
    advance_reporter(&mut app, reporter)?;

    let (departed_key, departure) = app
        .world()
        .resource::<ObservedEvents>()
        .departures
        .first()
        .cloned()
        .ok_or("the scripted scan change announced no departure")?;

    Ok(ObservedDeparture {
        key: departed_key,
        departure,
        retained_devices: app.world().resource::<Devices>().count(),
    })
}

/// A role that fails and restarts inside one frame must announce both moves.
///
/// The state is announced from the apply stage rather than from a mirrored component precisely so
/// this is visible: a once-per-frame mirror would compare `RoleState::Applying` against
/// `RoleState::Applying`, conclude nothing moved, and hide the failure that ran between them along
/// with the retry it caused.
#[test]
fn a_role_that_fails_and_restarts_in_one_frame_announces_both_moves() -> Result<(), Box<dyn Error>>
{
    let key = panel_key("CL15")?;
    let (mut app, reporter) = scripted_app(vec![scan![ScriptedDevice::present(key.clone())]])?;
    let role = RoleKey::new("panel")?;
    register_role(
        &mut app,
        &role,
        key,
        RecoveryPolicy::default(),
        RetryOn::Interval(Duration::ZERO),
        ApplyDeadline::ProcessDefault,
    )?;
    app.world_mut()
        .resource_mut::<DriverCalls>()
        .failures_remaining = 1;

    advance_reporter(&mut app, reporter)?;

    let announced_before = app.world().resource::<ObservedEvents>().role_state.len();
    app.update();

    assert!(matches!(
        &app.world().resource::<ObservedEvents>().role_state[announced_before..],
        [RoleState::Waiting, RoleState::Applying(_)]
    ));

    Ok(())
}

/// Start one apply under `apply_deadline` and report how far past the start its deadline sits.
///
/// Rounded to whole seconds because the stamp is `start + deadline` and the start is however many
/// milliseconds into the run the dispatch happened; the fact under test is which of the two
/// durations was used, not the frame it landed on.
fn rounded_deadline_gap(apply_deadline: ApplyDeadline) -> Result<Duration, Box<dyn Error>> {
    let key = panel_key("CL15")?;
    let (mut app, reporter) = scripted_app(vec![scan![ScriptedDevice::present(key.clone())]])?;
    let role = RoleKey::new("panel")?;
    register_role(
        &mut app,
        &role,
        key,
        RecoveryPolicy::default(),
        RetryOn::NewRevision,
        apply_deadline,
    )?;

    advance_reporter(&mut app, reporter)?;

    let started = app
        .world()
        .resource::<DriverCalls>()
        .started
        .first()
        .copied()
        .ok_or("the scripted driver was never dispatched")?;
    let startup = app.world().resource::<Time<Real>>().startup();
    let AttemptLookup::InFlight(attempt) = app.world().resource::<Attempts>().in_flight(started)
    else {
        return Err("the attempt ended before its deadline could be read".into());
    };

    Ok(Duration::from_secs(
        attempt.deadline.duration_since(startup).as_secs(),
    ))
}

/// Every event type must resolve through the registry, so a consumer can watch the whole lifecycle
/// over the Bevy Remote Protocol without a line of application code.
///
/// The workspace `bevy` enables `reflect_auto_register`, so this builds a bare app and reads the
/// registry rather than calling `App::register_type`.
#[test]
fn every_event_type_resolves_through_the_type_registry() {
    let app = App::new();
    let type_registry = app.world().resource::<AppTypeRegistry>().read();

    for type_id in [
        std::any::TypeId::of::<DeviceArrived>(),
        std::any::TypeId::of::<PresenceChanged>(),
        std::any::TypeId::of::<ClaimChanged>(),
        std::any::TypeId::of::<IdentityChanged>(),
        std::any::TypeId::of::<RoleStateChanged>(),
        std::any::TypeId::of::<RecoveryPolicyChanged>(),
        std::any::TypeId::of::<ReapplyConfiguration>(),
        std::any::TypeId::of::<DeviceDeparted>(),
        std::any::TypeId::of::<RetireRole>(),
        std::any::TypeId::of::<RoleAwaiting>(),
        std::any::TypeId::of::<RoleAvailable>(),
        std::any::TypeId::of::<ConfiguredDeviceConnectionChanged>(),
        std::any::TypeId::of::<UnregisteredSchemeReported>(),
        std::any::TypeId::of::<AttemptFinished>(),
        std::any::TypeId::of::<RetiredRoleAttemptEnded>(),
        std::any::TypeId::of::<CapabilitiesDisputed>(),
        std::any::TypeId::of::<DiscoveryProgressChanged>(),
        std::any::TypeId::of::<HardwareDiscoveryProgress>(),
        std::any::TypeId::of::<DiscoveryFinished>(),
        std::any::TypeId::of::<StartupDiscoveryChanged>(),
    ] {
        assert!(type_registry.get(type_id).is_some());
    }
    drop(type_registry);
}

/// Read how many of each observed event have arrived so far.
fn observed_counts(app: &App) -> (usize, usize, usize, usize, usize) {
    let observed = app.world().resource::<ObservedEvents>();
    (
        observed.presences.len(),
        observed.claims.len(),
        observed.arrivals.len(),
        observed.departures.len(),
        observed.role_state.len(),
    )
}
