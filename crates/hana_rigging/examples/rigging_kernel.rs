//! Headless smoke run of the rigging kernel: two reporters push overlapping whole-set scans into
//! a live Bevy `App`, and the kernel merges them into one device set.
//!
//! The example registers an identity scheme, registers two `DeviceReporter` implementations, drives
//! real frames until both have completed a scan, then reads the reconciled set back out of
//! `Devices`. It touches no window, renderer, filesystem, or network.

use std::error::Error;
use std::process::ExitCode;
use std::time::Duration;

use bevy::MinimalPlugins;
use bevy::app::App;
use bevy::prelude::World;
use hana_rigging::prelude::*;

const COLOR_MANAGEMENT_REPORTER: &str = "color-management";
const DESK_MONITOR: &str = "DESK-4K-0002";
const DISPLAY_SCHEME: &str = "example-edid-serial";
/// Frames the example may spend waiting for both reporters; discovery admits a bounded number of
/// jobs per frame, so a two-reporter startup takes more than one frame.
const FRAME_CEILING: u32 = 64;
const SHARED_PANEL: &str = "BUILT-IN-PANEL-0001";
const STAGE_PROJECTOR: &str = "STAGE-PROJECTOR-0003";
const WINDOW_SYSTEM_REPORTER: &str = "window-system";

/// Reports the same authored whole set on every scan, standing in for a platform enumeration call.
///
/// A real reporter would ask the operating system here. Every scan returns the reporter's *whole*
/// current set, because an omitted key is how the kernel learns a device departed.
struct FixedSetReporter {
    reported_keys: Vec<DeviceKey>,
}

impl DeviceReporter for FixedSetReporter {
    fn discover(&mut self) -> DiscoveryWork {
        let reported_keys = self.reported_keys.clone();

        DiscoveryWork::Immediate(MainThreadDiscoveryJob::new(move |_: &mut World| {
            DeviceScan::Complete(reported_keys.into_iter().map(present_display).collect())
        }))
    }
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
                 reporters, no duplicate keys"
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

    // Two independent platform sources that both see the built-in panel and disagree about nothing
    // else: each one also enumerates a display the other never sees.
    let reporters = vec![
        NamedReporter {
            name: WINDOW_SYSTEM_REPORTER,
            id:   app.add_device_reporter(
                FixedSetReporter {
                    reported_keys: vec![shared_panel.clone(), desk_monitor.clone()],
                },
                every_frame_registration(),
            ),
        },
        NamedReporter {
            name: COLOR_MANAGEMENT_REPORTER,
            id:   app.add_device_reporter(
                FixedSetReporter {
                    reported_keys: vec![shared_panel.clone(), stage_projector.clone()],
                },
                every_frame_registration(),
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
            key:                   desk_monitor,
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

            Ok(check_reconciled(devices, &displays))
        },
    }
}

/// Register a reporter that is due on every frame and whose first complete scan gates readiness.
fn every_frame_registration() -> ReporterRegistration {
    ReporterRegistration::required(
        DiscoveryCadence::Periodic {
            interval: Duration::ZERO,
        },
        ReporterCoverage::MatchingEvidenceOnly,
    )
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
                        "  {key} | {} | DeviceId({}) | {:?} | contributors: {}",
                        display.label,
                        device_id.get(),
                        reconciled_device_state.presence,
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
