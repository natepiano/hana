//! Headless smoke run of the rigging kernel: two reporters push overlapping whole-set scans into
//! a live Bevy `App`, and the kernel merges them into one device set.
//!
//! The example registers an identity scheme, registers two `DeviceReporter` implementations, one
//! authored role binding, and one authored inventory entry, drives real frames until both reporters
//! have completed a scan, then reads the reconciled set out of `Devices` and the role's binding
//! entity and its live device link out of `BindingEntities`. It then provokes a departure by having
//! one reporter stop naming a key in a later complete scan, and prints the connection conclusion
//! that moves as a result. It touches no window, renderer, filesystem, or network.

use std::error::Error;
use std::process::ExitCode;
use std::time::Duration;

use bevy::MinimalPlugins;
use bevy::app::App;
use bevy::ecs::reflect::ReflectComponent;
use bevy::ecs::relationship::Relationship;
use bevy::ecs::relationship::RelationshipTarget;
use bevy::prelude::Component;
use bevy::prelude::Reflect;
use bevy::prelude::World;
use hana_rigging::prelude::*;

const COLOR_MANAGEMENT_REPORTER: &str = "color-management";
/// Application role bound to the built-in panel, standing in for a window whose placement outlives
/// the display it was on.
const PANEL_ROLE: &str = "primary-window";
const DESK_MONITOR: &str = "DESK-4K-0002";
const DISPLAY_SCHEME: &str = "example-edid-serial";
/// Frames the example may spend waiting for both reporters; discovery admits a bounded number of
/// jobs per frame, so a two-reporter startup takes more than one frame.
const FRAME_CEILING: u32 = 64;
/// Completed scans the window-system reporter makes before it stops naming the desk monitor.
const SCANS_BEFORE_UNPLUG: u32 = 2;
const SHARED_PANEL: &str = "BUILT-IN-PANEL-0001";
const STAGE_PROJECTOR: &str = "STAGE-PROJECTOR-0003";
const WINDOW_SYSTEM_REPORTER: &str = "window-system";

/// Reports the same authored whole set on every scan, standing in for a platform enumeration call.
///
/// A real reporter would ask the operating system here. Every scan returns the reporter's *whole*
/// current set, because an omitted key is how the kernel learns a device departed.
struct FixedSetReporter {
    reported_keys:   Vec<DeviceKey>,
    withdrawal:      KeyWithdrawal,
    completed_scans: u32,
}

/// Whether this reporter eventually stops naming one of its keys, standing in for an unplug.
///
/// A departure is only observable because the whole set arrives every scan, so provoking one means
/// leaving a key out of a later complete report rather than sending a removal message.
enum KeyWithdrawal {
    /// The reporter names its whole authored set for the life of the run.
    Never,
    /// Once the reporter has completed `after_scans` scans it stops naming `key`.
    AfterScans {
        after_scans: u32,
        key:         DeviceKey,
    },
}

impl DeviceReporter for FixedSetReporter {
    fn discover(&mut self) -> DiscoveryWork {
        self.completed_scans += 1;
        let mut reported_keys = self.reported_keys.clone();
        if let KeyWithdrawal::AfterScans { after_scans, key } = &self.withdrawal
            && self.completed_scans > *after_scans
        {
            reported_keys.retain(|reported_key| reported_key != key);
        }

        DiscoveryWork::Immediate(MainThreadDiscoveryJob::new(move |_: &mut World| {
            DeviceScan::Complete(reported_keys.into_iter().map(present_display).collect())
        }))
    }
}

/// Endpoint driver registered only so the panel role has a real `DriverId` to name.
///
/// This example drives no hardware and nothing here dispatches to a driver, so every method
/// reports the outcome that says "this endpoint gave the kernel nothing" rather than pretending an
/// apply succeeded.
struct UnimplementedDriver;

impl EndpointDriver for UnimplementedDriver {
    type Configuration = PanelPlacement;

    fn capture(
        &mut self,
        _: &mut World,
        _: &DeviceEndpoint,
    ) -> CaptureOutcome<Self::Configuration> {
        CaptureOutcome::NotReadable
    }

    fn start_apply(
        &mut self,
        _: &mut World,
        _: &DeviceEndpoint,
        _: &Self::Configuration,
        _: AttemptId,
        _: ApplyPermit,
    ) {
    }

    fn poll(&mut self, _: &mut World, _: AttemptId) -> AttemptProgress {
        AttemptProgress::Finished(AttemptOutcome::Aborted)
    }
}

/// Placement this example's role would ask its driver for, standing in for a window rectangle.
#[derive(Component, Reflect)]
#[reflect(Component)]
struct PanelPlacement {
    left_pixels: i32,
    top_pixels:  i32,
}

/// One registered reporter and the name the report lines print beside its `ReporterId`.
struct NamedReporter {
    id:   ReporterId,
    name: &'static str,
}

/// One display this example reports, and how many reporters should end up contributing to it.
struct ReportedDisplay {
    key:                   DeviceKey,
    label:                 &'static str,
    expected_contributors: usize,
}

/// Whether every registered reporter has had a completed whole-set scan accepted.
#[derive(Clone, Copy, PartialEq, Eq)]
enum ScanCoverage {
    Pending,
    EveryReporterCompleted,
}

/// How the bounded frame loop ended.
enum ReporterStartup {
    /// Every reporter completed a scan, and the reconcile pass of this frame merged their sets.
    Completed { frames: u32 },
    /// The frame ceiling arrived first, so the kernel never reached the expected state.
    CeilingReached,
}

/// What checking the reconciled set against the reported overlap concluded.
enum SmokeCheck {
    Matched,
    Mismatched(Vec<String>),
}

fn main() -> ExitCode {
    match run() {
        Ok(SmokeCheck::Matched) => {
            println!(
                "OK — three reconciled devices, the shared panel carries two contributing \
                 reporters and a live ResolvedToDevice link, no duplicate keys, and the withdrawn \
                 desk monitor departed and left its authored inventory entry reading Absent"
            );
            ExitCode::SUCCESS
        },
        Ok(SmokeCheck::Mismatched(mismatches)) => {
            println!("FAILED — {}", mismatches.join("; "));
            ExitCode::FAILURE
        },
        Err(error) => {
            println!("FAILED — the kernel rejected this example's setup: {error}");
            ExitCode::FAILURE
        },
    }
}

fn run() -> Result<SmokeCheck, Box<dyn Error>> {
    let shared_panel = reported_display_key(SHARED_PANEL)?;
    let desk_monitor = reported_display_key(DESK_MONITOR)?;
    let stage_projector = reported_display_key(STAGE_PROJECTOR)?;

    let mut app = App::new();
    // An unregistered scheme is rejected at the ingest boundary, so the identity space the reported
    // keys name is registered before any reporter can report one.
    app.add_plugins(MinimalPlugins)
        .add_plugins(RiggingPlugin)
        .register_device_scheme(SchemeName::new(DISPLAY_SCHEME)?);

    // Binding registration is application work, not discovery work: this role is registered before
    // any reporter has run, and its binding entity is spawned by the first frame regardless.
    let panel_role = RoleKey::new(PANEL_ROLE)?;
    let panel_driver = app.add_endpoint_driver(UnimplementedDriver);
    app.world_mut()
        .resource_mut::<Bindings>()
        .register(panel_binding(
            panel_role.clone(),
            shared_panel.clone(),
            panel_driver,
        ))?;

    // Two independent platform sources that both see the built-in panel and disagree about nothing
    // else: each one also enumerates a display the other never sees.
    // Authoring the desk monitor into inventory neither enables a reporter nor creates a device:
    // it is what gives the unit a connection conclusion to move when discovery stops naming it.
    app.world_mut()
        .resource_mut::<HardwareInventory>()
        .configure(ConfiguredDevice {
            key:  desk_monitor.clone(),
            mode: ConfiguredDeviceMode::Managed,
        });

    let reporters = vec![
        NamedReporter {
            name: WINDOW_SYSTEM_REPORTER,
            id:   app.add_device_reporter(
                FixedSetReporter {
                    reported_keys:   vec![shared_panel.clone(), desk_monitor.clone()],
                    withdrawal:      KeyWithdrawal::AfterScans {
                        after_scans: SCANS_BEFORE_UNPLUG,
                        key:         desk_monitor.clone(),
                    },
                    completed_scans: 0,
                },
                every_frame_registration()?,
            ),
        },
        NamedReporter {
            name: COLOR_MANAGEMENT_REPORTER,
            id:   app.add_device_reporter(
                FixedSetReporter {
                    reported_keys:   vec![shared_panel.clone(), stage_projector.clone()],
                    withdrawal:      KeyWithdrawal::Never,
                    completed_scans: 0,
                },
                every_frame_registration()?,
            ),
        },
    ];

    let displays = vec![
        ReportedDisplay {
            key:                   shared_panel,
            label:                 "built-in panel (reported by both)",
            expected_contributors: 2,
        },
        ReportedDisplay {
            key:                   desk_monitor.clone(),
            label:                 "desk monitor (window-system only)",
            expected_contributors: 1,
        },
        ReportedDisplay {
            key:                   stage_projector,
            label:                 "stage projector (color-management only)",
            expected_contributors: 1,
        },
    ];

    for named_reporter in &reporters {
        println!(
            "registered reporter: {} {:?}",
            named_reporter.name, named_reporter.id
        );
    }

    match run_until_reporters_complete(&mut app, &reporters) {
        ReporterStartup::CeilingReached => Ok(SmokeCheck::Mismatched(vec![format!(
            "reached the {FRAME_CEILING}-frame ceiling before every reporter completed a scan"
        )])),
        ReporterStartup::Completed { frames } => {
            println!("frames until every reporter completed a scan: {frames}");
            let devices = app.world().resource::<Devices>();
            print_reconciled_devices(devices, &displays, &reporters);
            println!(
                "rigging revision: {}",
                app.world().resource::<RiggingRevision>().get()
            );
            let mut smoke_check = check_reconciled(devices, &displays);
            print_binding(app.world(), &panel_role, &mut smoke_check);
            print_connection(app.world(), &desk_monitor, "before the unplug");

            provoke_departure(&mut app, &desk_monitor, &displays, &mut smoke_check);

            Ok(smoke_check)
        },
    }
}

/// Drive frames until the withdrawn key leaves the reconciled set, then report what followed.
///
/// The departure is the whole point of the run: nothing tells the kernel a display was unplugged,
/// it simply stops appearing in a complete scan, and the authored inventory entry moves from
/// `Present` to `Absent` because the reporter that omitted it enumerates its identity space.
fn provoke_departure(
    app: &mut App,
    departing: &DeviceKey,
    displays: &[ReportedDisplay],
    smoke_check: &mut SmokeCheck,
) {
    for frame in 1..=FRAME_CEILING {
        app.update();
        if app.world().resource::<Devices>().resolve(departing) == DeviceResolution::NotResolved {
            println!("frames until the withdrawn key left the reconciled set: {frame}");
            let remaining: Vec<&ReportedDisplay> = displays
                .iter()
                .filter(|display| &display.key != departing)
                .collect();
            let devices = app.world().resource::<Devices>();
            println!(
                "reconciled devices after the departure: {}",
                devices.count()
            );
            if devices.count() != remaining.len() {
                record_mismatch(
                    smoke_check,
                    format!(
                        "expected {} reconciled devices after the departure, kernel retained {}",
                        remaining.len(),
                        devices.count()
                    ),
                );
            }
            print_connection(app.world(), departing, "after the unplug");
            let connection = app
                .world()
                .resource::<HardwareInventory>()
                .connection(departing);
            if connection != Ok(ConfiguredDeviceConnection::Absent) {
                record_mismatch(
                    smoke_check,
                    format!(
                        "expected the authored desk monitor to read Absent after the unplug, got \
                         {connection:?}"
                    ),
                );
            }

            return;
        }
    }

    record_mismatch(
        smoke_check,
        format!(
            "reached the {FRAME_CEILING}-frame ceiling before the withdrawn key left the \
             reconciled set"
        ),
    );
}

/// Print what passive evidence currently says about one authored inventory key.
fn print_connection(world: &World, device_key: &DeviceKey, when: &str) {
    match world.resource::<HardwareInventory>().connection(device_key) {
        Ok(connection) => println!(
            "  authored inventory {} | {when} | {connection:?}",
            describe_key(device_key)
        ),
        Err(error) => println!("  authored inventory read failed {when}: {error}"),
    }
}

/// Author one role that owns the whole built-in panel, with the default retention policy.
fn panel_binding(role: RoleKey, device: DeviceKey, driver: DriverId) -> Binding {
    Binding {
        role,
        endpoint: DeviceEndpoint {
            device,
            id: EndpointId::Whole,
        },
        driver,
        recovery: RecoveryPolicy::default(),
        retry: RetryOn::NewRevision,
        on_abort: OnAbort::default(),
        on_loss: OnSessionLoss::default(),
        state: RoleState::default(),
        requested: RequestedConfiguration::new(PanelPlacement {
            left_pixels: 0,
            top_pixels:  0,
        }),
        last_known_good: LastKnownGoodConfiguration::default(),
    }
}

/// Register a reporter that is due on every frame and whose first complete scan gates readiness.
///
/// The coverage is authoritative for this example's identity space: without it a reporter leaving a
/// key out of a complete scan would prove nothing, and the authored inventory entry could never
/// move off `NotObserved`.
fn every_frame_registration() -> Result<ReporterRegistration, Box<dyn Error>> {
    Ok(ReporterRegistration::required(
        DiscoveryCadence::Periodic {
            interval: Duration::ZERO,
        },
        ReporterCoverage::EstablishesAbsence(AuthoritativeReporterCoverage::one(
            CoveredDeviceIdentitySpace::ReportedScheme {
                kind:   DeviceKind::Display,
                scheme: SchemeName::new(DISPLAY_SCHEME)?,
            },
        )),
    ))
}

fn reported_display_key(value: &str) -> Result<DeviceKey, Box<dyn Error>> {
    Ok(DeviceKey {
        kind: DeviceKind::Display,
        id:   DeviceIdSource::Reported {
            scheme: SchemeName::new(DISPLAY_SCHEME)?,
            value:  ReportedId::new(value)?,
        },
    })
}

/// Build the record a reporter hands the kernel for one reachable display it can name durably.
fn present_display(device_key: DeviceKey) -> DeviceRecord {
    DeviceRecord {
        reported_as:  ReportedAs::Keyed(device_key),
        parent:       ReportedParent::Root,
        presence:     Presence::Present,
        claim:        Claim::NotApplicable,
        capabilities: Capabilities::new(),
        serial:       ReportedSerial::NotExposedByUnit,
        os_id:        OsDeviceId::PlatformReportedNothing,
        attachment:   AttachmentPath::PlatformHasNoConcept,
        descriptor:   DeviceDescriptor::PlatformReportedNothing,
    }
}

/// Drive frames until every reporter's whole set has been accepted, or the ceiling arrives.
///
/// `RiggingSystems::Reconcile` is chained after `RiggingSystems::Collect`, so the frame that
/// accepts the last outstanding scan is also the frame that merges it.
fn run_until_reporters_complete(app: &mut App, reporters: &[NamedReporter]) -> ReporterStartup {
    for frame in 1..=FRAME_CEILING {
        app.update();
        if scan_coverage(app.world(), reporters) == ScanCoverage::EveryReporterCompleted {
            return ReporterStartup::Completed { frames: frame };
        }
    }

    ReporterStartup::CeilingReached
}

fn scan_coverage(world: &World, reporters: &[NamedReporter]) -> ScanCoverage {
    let discovery_status = world.resource::<DiscoveryStatus>();
    for named_reporter in reporters {
        match discovery_status.reporter_status(named_reporter.id) {
            Ok(reporter_discovery_status)
                if matches!(
                    reporter_discovery_status.last_outcome,
                    LastDiscoveryOutcome::Succeeded { .. }
                ) => {},
            Ok(_) => return ScanCoverage::Pending,
            Err(error) => {
                println!("reporter status unavailable: {error}");
                return ScanCoverage::Pending;
            },
        }
    }

    ScanCoverage::EveryReporterCompleted
}

fn print_reconciled_devices(
    devices: &Devices,
    displays: &[ReportedDisplay],
    reporters: &[NamedReporter],
) {
    println!("reconciled devices: {}", devices.count());
    for display in displays {
        let key = describe_key(&display.key);
        match devices.resolve(&display.key) {
            DeviceResolution::NotResolved => {
                println!("  {key} | {} | no device handle was issued", display.label);
            },
            DeviceResolution::Resolved(device_id) => match devices.state(device_id) {
                DeviceStateLookup::Retired => {
                    println!(
                        "  {key} | {} | DeviceId({}) | retired",
                        display.label,
                        device_id.get()
                    );
                },
                DeviceStateLookup::Retained(reconciled_device_state) => {
                    println!(
                        "  {key} | {} | DeviceId({}) | {:?} | {:?} | contributors: {}",
                        display.label,
                        device_id.get(),
                        reconciled_device_state.presence,
                        reconciled_device_state.verdict,
                        contributor_names(&reconciled_device_state.contributors, reporters)
                    );
                },
            },
        }
    }

    let duplicate_keys = devices.duplicate_keys();
    if duplicate_keys.is_empty() {
        println!("duplicate keys: none");
    } else {
        for duplicate_key in duplicate_keys {
            println!("duplicate key: {}", describe_key(duplicate_key));
        }
    }
}

fn check_reconciled(devices: &Devices, displays: &[ReportedDisplay]) -> SmokeCheck {
    let mut mismatches = Vec::new();

    if devices.count() != displays.len() {
        mismatches.push(format!(
            "expected {} reconciled devices, kernel retained {}",
            displays.len(),
            devices.count()
        ));
    }
    for display in displays {
        let contributors = match devices.resolve(&display.key) {
            DeviceResolution::NotResolved => {
                mismatches.push(format!("{} resolved to no device", display.label));
                continue;
            },
            DeviceResolution::Resolved(device_id) => match devices.state(device_id) {
                DeviceStateLookup::Retired => {
                    mismatches.push(format!("{} resolved to a retired handle", display.label));
                    continue;
                },
                DeviceStateLookup::Retained(reconciled_device_state) => {
                    reconciled_device_state.contributors.len()
                },
            },
        };
        if contributors != display.expected_contributors {
            mismatches.push(format!(
                "{} expected {} contributing reporters, got {contributors}",
                display.label, display.expected_contributors
            ));
        }
    }
    if !devices.duplicate_keys().is_empty() {
        mismatches.push(format!(
            "expected no duplicate keys, kernel reported {}",
            devices.duplicate_keys().len()
        ));
    }

    if mismatches.is_empty() {
        SmokeCheck::Matched
    } else {
        SmokeCheck::Mismatched(mismatches)
    }
}

/// Print the role's binding entity and its live link to a device entity, if it has one.
///
/// A registered role reports no live link until reconciliation resolves its durable endpoint to a
/// device entity, which can be true even while its display is present. The example says which of
/// the two it observed rather than implying the link failed.
fn print_binding(world: &World, role: &RoleKey, smoke_check: &mut SmokeCheck) {
    let binding_entities = world.resource::<BindingEntities>();
    println!("binding entities: {}", binding_entities.count());
    match binding_entities.entity(role) {
        BindingEntityLookup::Unregistered => {
            record_mismatch(
                smoke_check,
                format!("role `{role}` has no binding entity after registration"),
            );
        },
        BindingEntityLookup::Registered(entity) => {
            match (
                world.get::<RecoveryPolicy>(entity),
                world.get::<RoleState>(entity),
            ) {
                (Some(recovery_policy), Some(role_state)) => {
                    println!("  role `{role}` | {entity} | {recovery_policy:?} | {role_state:?}");
                },
                _ => record_mismatch(
                    smoke_check,
                    format!(
                        "role `{role}`'s binding entity is missing a mirrored recovery policy or \
                         role state"
                    ),
                ),
            }
            match world.get::<ResolvedToDevice>(entity) {
                None => record_mismatch(
                    smoke_check,
                    format!(
                        "role `{role}` has no ResolvedToDevice link even though its endpoint names \
                         a device the kernel retains"
                    ),
                ),
                Some(resolved_to_device) => {
                    let device = resolved_to_device.get();
                    let resolved_bindings = world
                        .get::<ResolvedBindings>(device)
                        .map_or(0, |resolved_bindings| resolved_bindings.len());
                    println!(
                        "  role `{role}` | ResolvedToDevice({device}) | that device carries \
                         {resolved_bindings} resolved binding(s)"
                    );
                },
            }
        },
    }
}

fn record_mismatch(smoke_check: &mut SmokeCheck, mismatch: String) {
    match smoke_check {
        SmokeCheck::Matched => *smoke_check = SmokeCheck::Mismatched(vec![mismatch]),
        SmokeCheck::Mismatched(mismatches) => mismatches.push(mismatch),
    }
}

fn contributor_names(contributors: &[ReporterId], reporters: &[NamedReporter]) -> String {
    contributors
        .iter()
        .map(|contributor| {
            reporters
                .iter()
                .find(|named_reporter| named_reporter.id == *contributor)
                .map_or_else(
                    || format!("{contributor:?}"),
                    |named_reporter| format!("{} {contributor:?}", named_reporter.name),
                )
        })
        .collect::<Vec<_>>()
        .join(", ")
}

fn describe_key(device_key: &DeviceKey) -> String {
    match &device_key.id {
        DeviceIdSource::Reported { scheme, value } => {
            format!(
                "{:?} {}:{}",
                device_key.kind,
                scheme.as_str(),
                value.as_str()
            )
        },
        DeviceIdSource::Synthesized { digest } => {
            format!("{:?} synthesized:{digest:?}", device_key.kind)
        },
        DeviceIdSource::Authored { value } => {
            format!("{:?} authored:{}", device_key.kind, value.as_str())
        },
    }
}
