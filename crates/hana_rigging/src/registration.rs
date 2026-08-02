use std::time::Instant;

use bevy::app::App;
use bevy::prelude::Reflect;
use bevy::prelude::Resource;
use bevy::prelude::World;
use bevy::tasks::IoTaskPool;
use bevy::tasks::Task;
use bevy::tasks::block_on;
use bevy::tasks::poll_once;

use crate::CaptureRequest;
use crate::DeviceReporter;
use crate::DeviceScan;
use crate::DeviceSet;
use crate::DiscoveryBatchId;
use crate::DiscoveryCadence;
use crate::DiscoveryControl;
use crate::DiscoveryLimits;
use crate::DiscoveryProgress;
use crate::DiscoveryProgressSender;
use crate::DiscoverySchedulerState;
use crate::DiscoveryStatus;
use crate::DiscoveryWork;
use crate::DriverContractError;
use crate::LastDiscoveryOutcome;
use crate::PollRequest;
use crate::RegisteredSchemes;
use crate::ReporterActivation;
use crate::ReporterActivity;
use crate::ReporterId;
use crate::ReporterRegistration;
use crate::ReporterRevision;
use crate::SchemeName;
use crate::StartApplyRequest;
use crate::StartupDiscoveryState;
use crate::contract::DiscoveryProgressReceiver;
use crate::contract::DriverEntry;
use crate::contract::EndpointDriver;
use crate::contract::PendingDiscoveryProgress;
use crate::discovery::DiscoveryDirtyState;
use crate::discovery::DiscoveryRequest;
use crate::discovery::StartupRequirement;

/// Process-local driver handle that the driver registry issues in registration order.
///
/// `DriverId` has no public constructor because a binding receives it from
/// `RiggingAppExt::add_endpoint_driver`; allowing app code or a driver to mint one could route an
/// apply to a different registered implementation.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Reflect)]
#[reflect(opaque)]
pub struct DriverId(pub(crate) u32);

/// Proof that the kernel authorized an endpoint apply and selected its permitted purpose.
///
/// The private `Purpose` cases and the private constructors prevent a driver from manufacturing
/// permission for a device whose identity did not authorize it. `#[reflect(opaque)]` extends that
/// boundary to reflection: a dynamic reflected tuple cannot construct this token.
#[derive(Clone, Copy, Debug, Reflect)]
#[reflect(opaque)]
pub struct ApplyPermit(Purpose);

#[derive(Clone, Copy, Debug, Reflect)]
enum Purpose {
    /// The kernel may authorize a device with proven or authored identity for its bound work.
    InService,
    /// The kernel may authorize a device with restore-only identity to receive prior state only.
    RestoreOnly,
}

impl ApplyPermit {
    /// Report whether this token permits use for the device's current application role.
    ///
    /// A driver whose restore operation differs from in-service work checks this before choosing
    /// its hardware command; a driver with one command path can ignore the distinction.
    #[must_use]
    pub const fn allows_in_service_use(self) -> bool { matches!(self.0, Purpose::InService) }

    #[cfg_attr(
        not(test),
        expect(dead_code, reason = "used by the phase 10/11 kernel dispatch")
    )]
    pub(crate) const fn in_service() -> Self { Self(Purpose::InService) }

    #[cfg_attr(
        not(test),
        expect(dead_code, reason = "used by the phase 10/11 kernel dispatch")
    )]
    pub(crate) const fn restore_only() -> Self { Self(Purpose::RestoreOnly) }
}

/// Whether a reporter can supply an accepted complete device set to reconciliation.
#[cfg_attr(
    not(test),
    expect(
        dead_code,
        reason = "reconciliation distinguishes retained complete-set handoff states"
    )
)]
pub(crate) enum ReporterDeviceSetState<'a> {
    /// The requested `ReporterId` does not select a registry entry.
    NotRegistered,
    /// The reporter is registered but has no successfully accepted complete set.
    AwaitingCompleteSet,
    /// The reporter's latest successfully accepted complete set remains retained.
    Available(&'a DeviceSet),
}

/// Erased reporter implementations and their registry-owned discovery state.
///
/// The collect system borrows this resource out of `World` before it calls
/// `DeviceReporter::discover` or runs a returned `MainThreadDiscoveryJob`. The world-free reporter
/// method, the absent registry during main-thread enumeration, and the background job's sendable
/// closure prevent every discovery boundary from accessing its own registration.
#[derive(Resource, Default)]
pub(crate) struct Reporters {
    entries:    Vec<ReporterEntry>,
    next_batch: u64,
    next_id:    u32,
    changed:    Vec<ReporterId>,
    failures:   Vec<ReporterFailure>,
}

impl Reporters {
    pub(crate) fn add<Reporter>(
        &mut self,
        reporter: Reporter,
        reporter_registration: ReporterRegistration,
    ) -> ReporterId
    where
        Reporter: DeviceReporter,
    {
        let reporter_id = ReporterId(self.next_id);
        self.next_id += 1;
        self.entries.push(ReporterEntry::new(
            reporter,
            reporter_id,
            reporter_registration,
        ));

        reporter_id
    }

    pub(crate) fn collect(
        &mut self,
        world: &mut World,
        discovery_control: &mut DiscoveryControl,
        discovery_limits: &DiscoveryLimits,
        discovery_status: &mut DiscoveryStatus,
    ) {
        let now = Instant::now();
        self.poll_running(now);
        self.accept_completed(discovery_control, discovery_limits, discovery_status);
        self.refresh_startup(discovery_status);
        self.sync_activation(discovery_control);
        self.queue_due(now, discovery_control, discovery_status);
        self.admit(world, discovery_control, discovery_limits, discovery_status);
        self.refresh_startup(discovery_status);
        self.refresh_activity(now, discovery_status);
    }

    #[cfg_attr(
        not(test),
        expect(dead_code, reason = "reconciliation borrows retained complete sets")
    )]
    pub(crate) fn latest_device_set(&self, reporter: ReporterId) -> ReporterDeviceSetState<'_> {
        let Some(reporter_entry) = self
            .entries
            .iter()
            .find(|reporter_entry| reporter_entry.reporter_id == reporter)
        else {
            return ReporterDeviceSetState::NotRegistered;
        };

        match &reporter_entry.latest_set {
            RetainedDeviceSet::NotCompleted => ReporterDeviceSetState::AwaitingCompleteSet,
            RetainedDeviceSet::Complete { device_set, .. } => {
                ReporterDeviceSetState::Available(device_set)
            },
        }
    }

    #[cfg_attr(
        not(test),
        expect(
            dead_code,
            reason = "reconciliation drains changed reporter identifiers"
        )
    )]
    pub(crate) fn take_changed_reporters(&mut self) -> Vec<ReporterId> {
        std::mem::take(&mut self.changed)
    }

    #[cfg_attr(
        not(test),
        expect(dead_code, reason = "reconciliation drains retained reporter failures")
    )]
    pub(crate) fn take_reporter_failures(&mut self) -> Vec<ReporterFailure> {
        std::mem::take(&mut self.failures)
    }

    fn poll_running(&mut self, now: Instant) {
        for reporter_entry in &mut self.entries {
            reporter_entry.drain_progress();
            reporter_entry.poll(now);
        }
    }

    fn accept_completed(
        &mut self,
        discovery_control: &DiscoveryControl,
        discovery_limits: &DiscoveryLimits,
        discovery_status: &mut DiscoveryStatus,
    ) {
        let mut completed = Vec::new();
        for (index, reporter_entry) in self.entries.iter().enumerate() {
            match reporter_entry.completion_time() {
                ReporterCompletionTime::NoCompletedResult => {},
                ReporterCompletionTime::CompletedAt(completed_at) => {
                    completed.push((index, !reporter_entry.is_required(), completed_at));
                },
            }
        }
        completed.sort_by_key(|(_, optional, completed_at)| (*optional, *completed_at));

        for (index, _, _) in completed
            .into_iter()
            .take(discovery_limits.max_completions_per_frame().get())
        {
            let reporter_entry = &mut self.entries[index];
            let reporter_id = reporter_entry.reporter_id;
            let completed_discovery = match reporter_entry
                .take_completed(discovery_control.activation(reporter_id))
            {
                ReporterCompletionAcceptance::NoCompletedResult => continue,
                ReporterCompletionAcceptance::Accepted(completed_discovery) => completed_discovery,
            };

            let Ok(reporter_discovery_status) = discovery_status.reporter_status_mut(reporter_id)
            else {
                continue;
            };
            reporter_discovery_status.completed_batches += 1;

            match completed_discovery.scan {
                DeviceScan::Complete(devices) => {
                    reporter_entry.revision += 1;
                    reporter_entry.latest_set = RetainedDeviceSet::Complete {
                        device_set:   DeviceSet {
                            reporter: reporter_id,
                            devices,
                            revision: ReporterRevision::new(reporter_entry.revision),
                        },
                        completed_at: completed_discovery.completed_at,
                    };
                    self.changed.push(reporter_id);
                    reporter_discovery_status.last_outcome = LastDiscoveryOutcome::Succeeded {
                        batch:    completed_discovery.batch,
                        duration: completed_discovery
                            .completed_at
                            .duration_since(completed_discovery.started_at),
                    };
                    reporter_entry.schedule_after_completion(completed_discovery.completed_at);
                },
                DeviceScan::Failed(error) => {
                    reporter_discovery_status.last_outcome = LastDiscoveryOutcome::Failed {
                        batch:    completed_discovery.batch,
                        duration: completed_discovery
                            .completed_at
                            .duration_since(completed_discovery.started_at),
                        error:    error.clone(),
                    };
                    self.failures.push(ReporterFailure {
                        reporter: reporter_id,
                        error,
                    });
                    reporter_entry.schedule_after_completion(completed_discovery.completed_at);
                },
            }
        }
    }

    fn sync_activation(&mut self, discovery_control: &DiscoveryControl) {
        for reporter_entry in &mut self.entries {
            let reporter_id = reporter_entry.reporter_id;
            reporter_entry.sync_activation(discovery_control.activation(reporter_id));
        }
    }

    fn queue_due(
        &mut self,
        now: Instant,
        discovery_control: &mut DiscoveryControl,
        discovery_status: &mut DiscoveryStatus,
    ) {
        let startup_ready = matches!(discovery_status.startup, StartupDiscoveryState::Ready);
        let mut due_indexes = Vec::new();
        for (index, reporter_entry) in self.entries.iter_mut().enumerate() {
            if !reporter_entry.is_required() && !startup_ready {
                continue;
            }

            let reporter_id = reporter_entry.reporter_id;
            let requested = discovery_control.take_request(reporter_id);
            let dirty = discovery_control.take_dirty(reporter_id);
            if reporter_entry.record_due_signal(now, requested, dirty) {
                due_indexes.push(index);
            }
        }

        if due_indexes.is_empty() {
            return;
        }

        let batch = DiscoveryBatchId(self.next_batch);
        self.next_batch += 1;
        for index in due_indexes {
            let reporter_entry = &mut self.entries[index];
            reporter_entry.queue(batch, now);
            if let Ok(reporter_discovery_status) =
                discovery_status.reporter_status_mut(reporter_entry.reporter_id)
            {
                reporter_discovery_status.activity = ReporterActivity::Queued { batch };
            }
        }
    }

    fn admit(
        &mut self,
        world: &mut World,
        discovery_control: &DiscoveryControl,
        discovery_limits: &DiscoveryLimits,
        discovery_status: &mut DiscoveryStatus,
    ) {
        let mut queued = self
            .entries
            .iter()
            .enumerate()
            .filter(|(_, reporter_entry)| reporter_entry.can_prepare())
            .map(|(index, reporter_entry)| (index, !reporter_entry.is_required()))
            .collect::<Vec<_>>();
        queued.sort_by_key(|(_, optional)| *optional);

        for (index, optional) in queued {
            if optional && !matches!(discovery_status.startup, StartupDiscoveryState::Ready) {
                continue;
            }

            let reporter_entry = &mut self.entries[index];
            reporter_entry.prepare(world);
            self.start_prepared_background(
                index,
                discovery_control,
                discovery_limits,
                discovery_status,
            );
        }

        let prepared = self
            .entries
            .iter()
            .enumerate()
            .filter(|(_, reporter_entry)| reporter_entry.has_prepared_background())
            .map(|(index, reporter_entry)| (index, !reporter_entry.is_required()))
            .collect::<Vec<_>>();
        for (index, optional) in prepared {
            if optional && !matches!(discovery_status.startup, StartupDiscoveryState::Ready) {
                continue;
            }
            self.start_prepared_background(
                index,
                discovery_control,
                discovery_limits,
                discovery_status,
            );
        }
    }

    fn start_prepared_background(
        &mut self,
        index: usize,
        discovery_control: &DiscoveryControl,
        discovery_limits: &DiscoveryLimits,
        discovery_status: &mut DiscoveryStatus,
    ) {
        if !self.entries[index].has_prepared_background()
            || discovery_control.activation(self.entries[index].reporter_id)
                == ReporterActivation::Disabled
        {
            return;
        }
        if self.running_count() >= Self::effective_capacity(discovery_limits, discovery_status) {
            return;
        }
        self.entries[index].start_prepared_background();
    }

    fn effective_capacity(
        discovery_limits: &DiscoveryLimits,
        discovery_status: &mut DiscoveryStatus,
    ) -> usize {
        match discovery_limits.effective_max_concurrent_jobs() {
            Ok(capacity) => {
                discovery_status.scheduler = DiscoverySchedulerState::Available;
                capacity.get()
            },
            Err(error) => {
                discovery_status.scheduler = DiscoverySchedulerState::Failed { error };
                0
            },
        }
    }

    fn running_count(&self) -> usize {
        self.entries
            .iter()
            .filter(|reporter_entry| reporter_entry.is_running())
            .count()
    }

    fn refresh_startup(&self, discovery_status: &mut DiscoveryStatus) {
        for reporter_entry in self
            .entries
            .iter()
            .filter(|reporter_entry| reporter_entry.is_required())
        {
            let reporter_id = reporter_entry.reporter_id;
            let Ok(reporter_discovery_status) = discovery_status.reporter_status(reporter_id)
            else {
                continue;
            };
            if let LastDiscoveryOutcome::Failed { error, .. } =
                &reporter_discovery_status.last_outcome
            {
                discovery_status.startup = StartupDiscoveryState::BlockedByFailure {
                    reporter: reporter_id,
                    error:    error.clone(),
                };
                return;
            }
        }

        let all_required_reporters_succeeded = self
            .entries
            .iter()
            .filter(|reporter_entry| reporter_entry.is_required())
            .all(|reporter_entry| {
                discovery_status
                    .reporter_status(reporter_entry.reporter_id)
                    .is_ok_and(|reporter_discovery_status| {
                        matches!(
                            reporter_discovery_status.last_outcome,
                            LastDiscoveryOutcome::Succeeded { .. }
                        ) && matches!(
                            &reporter_entry.latest_set,
                            RetainedDeviceSet::Complete { .. }
                        )
                    })
            });
        discovery_status.startup = if all_required_reporters_succeeded {
            StartupDiscoveryState::Ready
        } else {
            StartupDiscoveryState::Discovering
        };
    }

    fn refresh_activity(&self, now: Instant, discovery_status: &mut DiscoveryStatus) {
        for reporter_entry in &self.entries {
            if let Ok(reporter_discovery_status) =
                discovery_status.reporter_status_mut(reporter_entry.reporter_id)
            {
                reporter_discovery_status.activity = reporter_entry.activity(now);
            }
        }
    }
}

/// Failure queue entry retained for reconciliation and later user-interface event emission.
#[cfg_attr(
    not(test),
    expect(dead_code, reason = "reconciliation drains each reporter failure")
)]
pub(crate) struct ReporterFailure {
    pub(crate) reporter: ReporterId,
    pub(crate) error:    crate::DeviceAccessError,
}

struct ReporterEntry {
    reporter:     Box<dyn DeviceReporter>,
    reporter_id:  ReporterId,
    registration: ReporterRegistration,
    revision:     u64,
    latest_set:   RetainedDeviceSet,
    next_due:     NextDue,
    state:        ReporterRunState,
}

enum RetainedDeviceSet {
    NotCompleted,
    Complete {
        device_set:   DeviceSet,
        #[cfg_attr(
            not(test),
            expect(
                dead_code,
                reason = "reconciliation consumes the retained set's actual completion clock"
            )
        )]
        completed_at: Instant,
    },
}

enum NextDue {
    NotScheduled,
    At(Instant),
}

enum ReporterRunState {
    Disabled,
    Idle {
        rerun: RerunRequest,
    },
    Queued {
        batch:     DiscoveryBatchId,
        queued_at: Instant,
        rerun:     RerunRequest,
        pending:   PendingDiscovery,
    },
    Running {
        task:                        Task<DeviceScan>,
        batch:                       DiscoveryBatchId,
        started_at:                  Instant,
        discovery_progress_receiver: DiscoveryProgressReceiver,
        progress:                    DiscoveryProgress,
        rerun:                       RerunRequest,
    },
}

enum PendingDiscovery {
    Admission,
    Background(crate::DiscoveryJob),
    Completed {
        scan:         DeviceScan,
        completed_at: Instant,
    },
}

#[derive(Clone, Copy)]
enum RerunRequest {
    NotRequested,
    Requested,
}

struct CompletedDiscovery {
    scan:         DeviceScan,
    batch:        DiscoveryBatchId,
    started_at:   Instant,
    completed_at: Instant,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ReporterCompletionTime {
    NoCompletedResult,
    CompletedAt(Instant),
}

enum ReporterCompletionAcceptance {
    NoCompletedResult,
    Accepted(CompletedDiscovery),
}

impl ReporterEntry {
    fn new<Reporter>(
        reporter: Reporter,
        reporter_id: ReporterId,
        registration: ReporterRegistration,
    ) -> Self
    where
        Reporter: DeviceReporter,
    {
        let state = match registration.activation() {
            ReporterActivation::Enabled => ReporterRunState::Idle {
                rerun: RerunRequest::NotRequested,
            },
            ReporterActivation::Disabled => ReporterRunState::Disabled,
        };
        Self {
            reporter: Box::new(reporter),
            reporter_id,
            registration,
            revision: 0,
            latest_set: RetainedDeviceSet::NotCompleted,
            next_due: NextDue::NotScheduled,
            state,
        }
    }

    fn is_required(&self) -> bool {
        self.registration.requirement() == StartupRequirement::Required
    }

    const fn completion_time(&self) -> ReporterCompletionTime {
        match &self.state {
            ReporterRunState::Queued {
                pending: PendingDiscovery::Completed { completed_at, .. },
                ..
            } => ReporterCompletionTime::CompletedAt(*completed_at),
            ReporterRunState::Disabled
            | ReporterRunState::Idle { .. }
            | ReporterRunState::Queued { .. }
            | ReporterRunState::Running { .. } => ReporterCompletionTime::NoCompletedResult,
        }
    }

    fn take_completed(&mut self, activation: ReporterActivation) -> ReporterCompletionAcceptance {
        let state = std::mem::replace(&mut self.state, ReporterRunState::Disabled);
        let ReporterRunState::Queued {
            batch,
            queued_at,
            rerun,
            pending: PendingDiscovery::Completed { scan, completed_at },
        } = state
        else {
            self.state = state;
            return ReporterCompletionAcceptance::NoCompletedResult;
        };

        self.state = match activation {
            ReporterActivation::Disabled => ReporterRunState::Disabled,
            ReporterActivation::Enabled => ReporterRunState::Idle { rerun },
        };
        ReporterCompletionAcceptance::Accepted(CompletedDiscovery {
            scan,
            batch,
            started_at: queued_at,
            completed_at,
        })
    }

    fn schedule_after_completion(&mut self, completed_at: Instant) {
        self.next_due = match self.registration.cadence() {
            DiscoveryCadence::OnDemand => NextDue::NotScheduled,
            DiscoveryCadence::EventDriven { backstop } => NextDue::At(completed_at + *backstop),
            DiscoveryCadence::Periodic { interval } => NextDue::At(completed_at + *interval),
        };
    }

    fn sync_activation(&mut self, activation: ReporterActivation) {
        match (activation, &self.state) {
            (ReporterActivation::Enabled, ReporterRunState::Disabled) => {
                self.state = ReporterRunState::Idle {
                    rerun: RerunRequest::NotRequested,
                };
            },
            (ReporterActivation::Disabled, ReporterRunState::Running { .. })
            | (ReporterActivation::Enabled, _) => {},
            (ReporterActivation::Disabled, _) => self.state = ReporterRunState::Disabled,
        }
    }

    fn record_due_signal(
        &mut self,
        now: Instant,
        requested: DiscoveryRequest,
        dirty: DiscoveryDirtyState,
    ) -> bool {
        let cadence_due = matches!(self.next_due, NextDue::At(deadline) if deadline <= now);
        let signalled = matches!(requested, crate::discovery::DiscoveryRequest::Requested)
            || matches!(dirty, crate::discovery::DiscoveryDirtyState::Dirty)
            || cadence_due;

        match &mut self.state {
            ReporterRunState::Idle { rerun } => match rerun {
                RerunRequest::Requested => {
                    *rerun = RerunRequest::NotRequested;
                    true
                },
                RerunRequest::NotRequested => signalled,
            },
            ReporterRunState::Queued {
                pending: PendingDiscovery::Completed { .. },
                rerun,
                ..
            }
            | ReporterRunState::Running { rerun, .. } => {
                if signalled {
                    *rerun = RerunRequest::Requested;
                }

                false
            },
            ReporterRunState::Queued { .. } | ReporterRunState::Disabled => false,
        }
    }

    fn queue(&mut self, batch: DiscoveryBatchId, queued_at: Instant) {
        if matches!(self.next_due, NextDue::At(deadline) if deadline <= queued_at) {
            self.next_due = NextDue::NotScheduled;
        }
        self.state = ReporterRunState::Queued {
            batch,
            queued_at,
            rerun: RerunRequest::NotRequested,
            pending: PendingDiscovery::Admission,
        };
    }

    const fn can_prepare(&self) -> bool {
        matches!(
            self.state,
            ReporterRunState::Queued {
                pending: PendingDiscovery::Admission,
                ..
            }
        )
    }

    fn prepare(&mut self, world: &mut World) {
        let discovery_work = self.reporter.discover();
        let ReporterRunState::Queued { pending, .. } = &mut self.state else {
            return;
        };
        *pending = match discovery_work {
            DiscoveryWork::Immediate(main_thread_discovery_job) => PendingDiscovery::Completed {
                scan:         main_thread_discovery_job.run(world),
                completed_at: Instant::now(),
            },
            DiscoveryWork::Background(discovery_job) => PendingDiscovery::Background(discovery_job),
        };
    }

    const fn has_prepared_background(&self) -> bool {
        matches!(
            self.state,
            ReporterRunState::Queued {
                pending: PendingDiscovery::Background(_),
                ..
            }
        )
    }

    fn start_prepared_background(&mut self) { self.start_prepared_background_with(Instant::now); }

    fn start_prepared_background_with(&mut self, sample_started_at: impl FnOnce() -> Instant) {
        let state = std::mem::replace(&mut self.state, ReporterRunState::Disabled);
        let ReporterRunState::Queued {
            batch,
            rerun,
            pending: PendingDiscovery::Background(discovery_job),
            ..
        } = state
        else {
            self.state = state;
            return;
        };
        let (discovery_progress_sender, discovery_progress_receiver) =
            DiscoveryProgressSender::scheduler_mailbox();
        let started_at = sample_started_at();
        let task =
            IoTaskPool::get().spawn(async move { discovery_job.run(discovery_progress_sender) });
        self.state = ReporterRunState::Running {
            task,
            batch,
            started_at,
            discovery_progress_receiver,
            progress: DiscoveryProgress::Indeterminate,
            rerun,
        };
    }

    fn drain_progress(&mut self) {
        let ReporterRunState::Running {
            discovery_progress_receiver,
            progress,
            ..
        } = &mut self.state
        else {
            return;
        };
        if let PendingDiscoveryProgress::Latest(discovery_progress) =
            discovery_progress_receiver.take_latest()
        {
            *progress = discovery_progress;
        }
    }

    fn poll(&mut self, now: Instant) {
        let is_complete = match &mut self.state {
            ReporterRunState::Running { task, .. } => block_on(poll_once(task)),
            ReporterRunState::Disabled
            | ReporterRunState::Idle { .. }
            | ReporterRunState::Queued { .. } => None,
        };
        let Some(scan) = is_complete else {
            return;
        };

        let state = std::mem::replace(&mut self.state, ReporterRunState::Disabled);
        let ReporterRunState::Running {
            batch,
            started_at,
            rerun,
            ..
        } = state
        else {
            self.state = state;
            return;
        };
        self.state = ReporterRunState::Queued {
            batch,
            queued_at: started_at,
            rerun,
            pending: PendingDiscovery::Completed {
                scan,
                completed_at: now,
            },
        };
    }

    const fn is_running(&self) -> bool { matches!(self.state, ReporterRunState::Running { .. }) }

    fn activity(&self, now: Instant) -> ReporterActivity {
        match &self.state {
            ReporterRunState::Disabled => ReporterActivity::Disabled,
            ReporterRunState::Idle { .. } => ReporterActivity::Idle,
            ReporterRunState::Queued { batch, .. } => ReporterActivity::Queued { batch: *batch },
            ReporterRunState::Running {
                batch,
                started_at,
                progress,
                ..
            } => ReporterActivity::Running {
                batch:    *batch,
                elapsed:  now.duration_since(*started_at),
                progress: progress.clone(),
            },
        }
    }
}

/// Erased endpoint drivers that kernel systems borrow out of `World` for capture and apply.
///
/// Kernel systems call the crate-private dispatch methods through `World::resource_scope`.
/// Endpoint driver implementations must not access `Drivers` from their own trait methods because
/// the resource is absent for that call. Every apply still requires an `ApplyPermit`, whose private
/// construction prevents application code from authorizing a device by selecting a driver route.
#[derive(Resource, Default)]
pub(crate) struct Drivers {
    drivers: Vec<DriverEntry>,
    next_id: u32,
}

impl Drivers {
    pub(crate) fn add<Driver>(&mut self, driver: Driver) -> DriverId
    where
        Driver: EndpointDriver,
    {
        let driver_id = DriverId(self.next_id);
        self.next_id += 1;
        self.drivers.push(DriverEntry::new(driver));

        driver_id
    }

    /// Ask one registered driver to capture its endpoint configuration through erased dispatch.
    ///
    /// Kernel capture systems call this inside `World::resource_scope` after checking endpoint
    /// presence and in-flight attempts. The driver contract owns those checks, so callers outside
    /// the kernel should use their own endpoint state instead.
    ///
    /// # Errors
    ///
    /// Returns `DriverContractError::DriverNotRegistered` when `driver_id` is not in this
    /// process's registry, or another `DriverContractError` when erased dispatch cannot recover
    /// the typed driver boundary.
    #[cfg_attr(
        not(test),
        expect(dead_code, reason = "used by the phase 10/11 kernel dispatch")
    )]
    pub(crate) fn capture(
        &mut self,
        world: &mut World,
        capture_request: CaptureRequest<'_>,
    ) -> Result<crate::CaptureOutcome<crate::LastKnownGoodConfiguration>, crate::DriverContractError>
    {
        let CaptureRequest {
            role: _,
            driver,
            endpoint,
        } = capture_request;
        self.get_mut(driver)
            .ok_or(DriverContractError::DriverNotRegistered { driver_id: driver })?
            .capture(world, endpoint)
    }

    /// Start one authorized endpoint apply through the driver selected by `driver_id`.
    ///
    /// Kernel attempt systems call this inside `World::resource_scope`; the `ApplyPermit` must be
    /// the same token stored on that attempt, so a driver receives the exact authorization the
    /// kernel recorded.
    ///
    /// # Errors
    ///
    /// Returns `DriverContractError::DriverNotRegistered` when `driver_id` is not in this
    /// process's registry, or `DriverContractError::ConfigurationTypeMismatch` when the erased
    /// configuration belongs to a different driver's concrete type.
    #[cfg_attr(
        not(test),
        expect(dead_code, reason = "used by the phase 10/11 kernel dispatch")
    )]
    pub(crate) fn start_apply(
        &mut self,
        world: &mut World,
        start_apply_request: StartApplyRequest<'_>,
    ) -> Result<(), crate::DriverContractError> {
        let StartApplyRequest {
            binding,
            configuration_source,
            attempt,
            permit,
        } = start_apply_request;
        let driver = binding.driver;
        let start_apply_result = {
            let endpoint = &binding.endpoint;
            let configuration = configuration_source.configuration(binding).map_err(|_| {
                DriverContractError::LastKnownGoodConfigurationUnavailable {
                    role: binding.role.clone(),
                }
            })?;
            self.get_mut(driver)
                .ok_or(DriverContractError::DriverNotRegistered { driver_id: driver })?
                .start_apply(world, endpoint, configuration, attempt, permit)
        };
        if start_apply_result.is_ok() {
            binding.state = crate::RoleState::Applying(attempt);
        }

        start_apply_result
    }

    /// Poll one in-flight apply through the driver selected by `driver_id`.
    ///
    /// Kernel attempt systems call this inside `World::resource_scope` after checking that the
    /// attempt still addresses the same device and rigging revision.
    ///
    /// # Errors
    ///
    /// Returns `DriverContractError::DriverNotRegistered` when `driver_id` is not in this
    /// process's registry, or another `DriverContractError` when erased dispatch cannot recover
    /// the typed driver boundary.
    #[cfg_attr(
        not(test),
        expect(dead_code, reason = "used by the phase 10/11 kernel dispatch")
    )]
    pub(crate) fn poll(
        &mut self,
        world: &mut World,
        poll_request: PollRequest<'_>,
    ) -> Result<crate::AttemptProgress, crate::DriverContractError> {
        let PollRequest {
            role: _,
            driver,
            endpoint: _,
            attempt,
        } = poll_request;
        self.get_mut(driver)
            .ok_or(DriverContractError::DriverNotRegistered { driver_id: driver })?
            .poll(world, attempt)
    }

    fn get_mut(&mut self, driver_id: DriverId) -> Option<&mut DriverEntry> {
        self.drivers.get_mut(usize::try_from(driver_id.0).ok()?)
    }
}

/// Adds Hana Rigging reporters, endpoint drivers, and identity schemes to a Bevy `App`.
///
/// Bevy's `App` cannot receive inherent methods from this crate, so integration crates import
/// `RiggingAppExt` before registering their hardware-specific reporter or driver implementation.
pub trait RiggingAppExt {
    /// Register one reporter with its startup and cadence policy, then return its process-local id.
    ///
    /// This initializes the reporter registry and retained discovery resources itself, so adding a
    /// reporter before `RiggingPlugin` does not make plugin insertion order control whether the
    /// reporter can register.
    fn add_device_reporter<Reporter>(
        &mut self,
        reporter: Reporter,
        reporter_registration: ReporterRegistration,
    ) -> ReporterId
    where
        Reporter: DeviceReporter;

    /// Register one endpoint driver and return the process-local id that bindings use for routing.
    ///
    /// This initializes `Drivers` itself, so adding a driver before `RiggingPlugin` does not make
    /// plugin insertion order control whether the driver can register.
    fn add_endpoint_driver<Driver>(&mut self, driver: Driver) -> DriverId
    where
        Driver: EndpointDriver;

    /// Record a reportable device-identity scheme and return this app for further setup.
    ///
    /// The method initializes `RegisteredSchemes` before recording `name`, so an integration
    /// plugin can register its scheme before or after `RiggingPlugin`. Repeated names stay valid:
    /// two reporters using one scheme assert that their reported values are comparable.
    fn register_device_scheme(&mut self, name: SchemeName) -> &mut Self;
}

impl RiggingAppExt for App {
    fn add_device_reporter<Reporter>(
        &mut self,
        reporter: Reporter,
        reporter_registration: ReporterRegistration,
    ) -> ReporterId
    where
        Reporter: DeviceReporter,
    {
        self.init_resource::<DiscoveryControl>()
            .init_resource::<DiscoveryLimits>()
            .init_resource::<DiscoveryStatus>()
            .init_resource::<Reporters>();
        let reporter_id = self
            .world_mut()
            .resource_mut::<Reporters>()
            .add(reporter, reporter_registration.clone());
        self.world_mut()
            .resource_mut::<DiscoveryControl>()
            .register(reporter_id, &reporter_registration);
        self.world_mut()
            .resource_mut::<DiscoveryStatus>()
            .register(reporter_id, &reporter_registration);

        reporter_id
    }

    fn add_endpoint_driver<Driver>(&mut self, driver: Driver) -> DriverId
    where
        Driver: EndpointDriver,
    {
        self.init_resource::<Drivers>();
        self.world_mut().resource_mut::<Drivers>().add(driver)
    }

    fn register_device_scheme(&mut self, name: SchemeName) -> &mut Self {
        self.init_resource::<RegisteredSchemes>();
        self.world_mut()
            .resource_mut::<RegisteredSchemes>()
            .register(name);
        self
    }
}

#[cfg(test)]
#[allow(
    clippy::expect_used,
    reason = "tests should panic on unexpected values"
)]
mod tests {
    use std::collections::VecDeque;
    use std::error::Error;
    use std::num::NonZeroU32;
    use std::num::NonZeroUsize;
    use std::sync::Arc;
    use std::sync::Mutex;
    use std::sync::atomic::AtomicBool;
    use std::sync::atomic::AtomicUsize;
    use std::sync::atomic::Ordering;
    use std::sync::mpsc::Receiver;
    use std::sync::mpsc::Sender;
    use std::sync::mpsc::TryRecvError;
    use std::sync::mpsc::channel;
    use std::thread::JoinHandle;
    use std::time::Duration;
    use std::time::Instant;

    use bevy::app::App;
    use bevy::app::Plugin;
    use bevy::ecs::reflect::ReflectComponent;
    use bevy::prelude::Component;
    use bevy::prelude::Reflect;
    use bevy::prelude::World;
    use bevy::reflect::FromReflect;
    use bevy::reflect::tuple_struct::DynamicTupleStruct;
    use bevy::tasks::IoTaskPool;
    use bevy::tasks::TaskPoolBuilder;

    use super::ApplyPermit;
    use super::DriverId;
    use super::Drivers;
    use super::NextDue;
    use super::PendingDiscovery;
    use super::Purpose;
    use super::ReporterCompletionAcceptance;
    use super::ReporterCompletionTime;
    use super::ReporterDeviceSetState;
    use super::ReporterEntry;
    use super::ReporterId;
    use super::ReporterRunState;
    use super::Reporters;
    use super::RerunRequest;
    use super::RetainedDeviceSet;
    use super::RiggingAppExt;
    use crate::AttachmentPath;
    use crate::AttemptId;
    use crate::AttemptOutcome;
    use crate::AttemptProgress;
    use crate::Binding;
    use crate::Bindings;
    use crate::Capabilities;
    use crate::CaptureOutcome;
    use crate::Claim;
    use crate::DeviceAccessError;
    use crate::DeviceDescriptor;
    use crate::DeviceEndpoint;
    use crate::DeviceIdSource;
    use crate::DeviceKey;
    use crate::DeviceKind;
    use crate::DeviceRecord;
    use crate::DeviceReporter;
    use crate::DeviceScan;
    use crate::DeviceSet;
    use crate::DiscoveryBatchId;
    use crate::DiscoveryCadence;
    use crate::DiscoveryControl;
    use crate::DiscoveryJob;
    use crate::DiscoveryLimits;
    use crate::DiscoveryProgress;
    use crate::DiscoveryStatus;
    use crate::DiscoveryWork;
    use crate::EndpointDriver;
    use crate::EndpointId;
    use crate::HardwareInventory;
    use crate::LastDiscoveryOutcome;
    use crate::LastKnownGoodConfiguration;
    use crate::MainThreadDiscoveryJob;
    use crate::OnAbort;
    use crate::OnSessionLoss;
    use crate::OsDeviceId;
    use crate::Presence;
    use crate::RecoveryPolicy;
    use crate::RegisteredSchemes;
    use crate::ReportedAs;
    use crate::ReportedId;
    use crate::ReportedSerial;
    use crate::ReporterActivation;
    use crate::ReporterActivity;
    use crate::ReporterCoverage;
    use crate::ReporterRegistration;
    use crate::RequestedConfiguration;
    use crate::RetryOn;
    use crate::RiggingPlugin;
    use crate::RoleKey;
    use crate::RoleState;
    use crate::RoleView;
    use crate::SchemeName;
    use crate::StartupDiscoveryState;
    use crate::discovery::DiscoveryDirtyState;
    use crate::discovery::DiscoveryRequest;

    struct CountingReporter {
        scans: Arc<AtomicUsize>,
    }

    impl DeviceReporter for CountingReporter {
        fn discover(&mut self) -> DiscoveryWork {
            self.scans.fetch_add(1, Ordering::Relaxed);
            DiscoveryWork::Immediate(MainThreadDiscoveryJob::new(|_| {
                DeviceScan::Complete(Vec::new())
            }))
        }
    }

    struct RecordReporter {
        scans: Arc<AtomicUsize>,
    }

    struct OrderedReporter {
        name:        &'static str,
        discoveries: Arc<Mutex<Vec<&'static str>>>,
    }

    impl DeviceReporter for OrderedReporter {
        fn discover(&mut self) -> DiscoveryWork {
            self.discoveries
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .push(self.name);

            DiscoveryWork::Immediate(MainThreadDiscoveryJob::new(|_| {
                DeviceScan::Complete(Vec::new())
            }))
        }
    }

    struct SequenceReporter {
        scans: VecDeque<DeviceScan>,
    }

    struct BackgroundReporter {
        discoveries: Arc<AtomicUsize>,
    }

    impl DeviceReporter for BackgroundReporter {
        fn discover(&mut self) -> DiscoveryWork {
            let discoveries = Arc::clone(&self.discoveries);
            DiscoveryWork::Background(DiscoveryJob::new(move |discovery_progress_sender| {
                discovery_progress_sender
                    .send(DiscoveryProgress::Measured {
                        completed: 1,
                        total:     NonZeroU32::MIN,
                    })
                    .expect("scheduler must retain a running job's progress receiver");
                discoveries.fetch_add(1, Ordering::Relaxed);

                DeviceScan::Complete(Vec::new())
            }))
        }
    }

    struct CountingBackgroundReporter {
        discoveries: Arc<AtomicUsize>,
    }

    impl DeviceReporter for CountingBackgroundReporter {
        fn discover(&mut self) -> DiscoveryWork {
            self.discoveries.fetch_add(1, Ordering::Relaxed);
            DiscoveryWork::Background(DiscoveryJob::new(|_| DeviceScan::Complete(Vec::new())))
        }
    }

    struct BackgroundJobGate {
        reporter_name: &'static str,
        started:       Sender<&'static str>,
        released:      Arc<AtomicBool>,
        progress:      DiscoveryProgress,
    }

    struct GatedBackgroundReporter {
        background_job_gate: Arc<BackgroundJobGate>,
    }

    impl DeviceReporter for GatedBackgroundReporter {
        fn discover(&mut self) -> DiscoveryWork {
            gated_discovery_work(Arc::clone(&self.background_job_gate))
        }
    }

    #[derive(Clone, Copy)]
    enum FirstDiscoveryOutcome {
        Succeeded,
        Failed,
    }

    #[derive(Clone, Copy)]
    enum RequiredReporterRegistrationOrder {
        FailureBeforeIncomplete,
        IncompleteBeforeFailure,
    }

    #[derive(Clone, Copy)]
    enum ReporterActivityExpectation {
        Disabled,
        Idle,
        Queued,
        Running,
    }

    #[derive(Debug)]
    enum DeviceSetUnavailable {
        ReporterNotRegistered,
        AwaitingCompleteSet,
    }

    struct OutcomeThenGatedReporter {
        first_discovery_outcome: FirstDiscoveryOutcome,
        discoveries:             Arc<AtomicUsize>,
        runs:                    usize,
        background_job_gate:     Arc<BackgroundJobGate>,
    }

    impl DeviceReporter for OutcomeThenGatedReporter {
        fn discover(&mut self) -> DiscoveryWork {
            self.discoveries.fetch_add(1, Ordering::Relaxed);
            self.runs += 1;
            if self.runs == 1 {
                let device_scan = match self.first_discovery_outcome {
                    FirstDiscoveryOutcome::Succeeded => DeviceScan::Complete(Vec::new()),
                    FirstDiscoveryOutcome::Failed => {
                        DeviceScan::Failed(DeviceAccessError::Transport {
                            detail: String::from("test discovery failure"),
                        })
                    },
                };
                return DiscoveryWork::Immediate(MainThreadDiscoveryJob::new(move |_| device_scan));
            }

            gated_discovery_work(Arc::clone(&self.background_job_gate))
        }
    }

    struct BackgroundJobRelease {
        released: Arc<AtomicBool>,
    }

    impl BackgroundJobRelease {
        fn release(self) { self.released.store(true, Ordering::Release); }
    }

    impl Drop for BackgroundJobRelease {
        fn drop(&mut self) { self.released.store(true, Ordering::Release); }
    }

    fn background_job_gate(
        reporter_name: &'static str,
        started: Sender<&'static str>,
        progress: DiscoveryProgress,
    ) -> (Arc<BackgroundJobGate>, BackgroundJobRelease) {
        let released = Arc::new(AtomicBool::new(false));
        (
            Arc::new(BackgroundJobGate {
                reporter_name,
                started,
                released: Arc::clone(&released),
                progress,
            }),
            BackgroundJobRelease { released },
        )
    }

    fn gated_discovery_work(background_job_gate: Arc<BackgroundJobGate>) -> DiscoveryWork {
        DiscoveryWork::Background(DiscoveryJob::new(move |discovery_progress_sender| {
            discovery_progress_sender
                .send(background_job_gate.progress.clone())
                .expect("scheduler must retain a running job's progress receiver");
            background_job_gate
                .started
                .send(background_job_gate.reporter_name)
                .expect("test must retain the job-start receiver");
            while !background_job_gate.released.load(Ordering::Acquire) {
                std::thread::yield_now();
            }

            DeviceScan::Complete(Vec::new())
        }))
    }

    fn wait_for_started_jobs(
        started: &Receiver<&'static str>,
        expected_count: usize,
    ) -> Vec<&'static str> {
        const JOB_START_TIMEOUT: Duration = Duration::from_secs(5);

        let mut reporter_names = (0..expected_count)
            .map(|_| {
                started
                    .recv_timeout(JOB_START_TIMEOUT)
                    .expect("admitted discovery job must reach its deterministic gate")
            })
            .collect::<Vec<_>>();
        reporter_names.sort_unstable();
        reporter_names
    }

    fn assert_no_additional_job_started(started: &Receiver<&'static str>) {
        assert_eq!(started.try_recv(), Err(TryRecvError::Empty));
    }

    fn release_jobs_after_start(
        started: Receiver<&'static str>,
        expected_count: usize,
        releases: Vec<BackgroundJobRelease>,
    ) -> JoinHandle<(Vec<&'static str>, Receiver<&'static str>)> {
        std::thread::spawn(move || {
            let reporter_names = wait_for_started_jobs(&started, expected_count);
            for release in releases {
                release.release();
            }
            (reporter_names, started)
        })
    }

    fn update_until_completed_batches(app: &mut App, reporter: ReporterId, completed_batches: u64) {
        const MAX_UPDATES: usize = 100;

        for _ in 0..MAX_UPDATES {
            app.update();
            let reporter_discovery_status = app
                .world()
                .resource::<DiscoveryStatus>()
                .reporter_status(reporter)
                .expect("registered reporter must retain status");
            if reporter_discovery_status.completed_batches >= completed_batches {
                return;
            }
            std::thread::yield_now();
        }

        assert_eq!(
            app.world()
                .resource::<DiscoveryStatus>()
                .reporter_status(reporter)
                .expect("registered reporter must retain status")
                .completed_batches,
            completed_batches
        );
    }

    fn assert_successful_reporter_status(
        app: &App,
        reporter: ReporterId,
        activity_expectation: ReporterActivityExpectation,
        completed_batches: u64,
    ) {
        let reporter_discovery_status = app
            .world()
            .resource::<DiscoveryStatus>()
            .reporter_status(reporter)
            .expect("registered reporter must retain status");
        assert!(matches!(
            reporter_discovery_status.last_outcome,
            LastDiscoveryOutcome::Succeeded { .. }
        ));
        assert_eq!(
            reporter_discovery_status.completed_batches,
            completed_batches
        );
        match activity_expectation {
            ReporterActivityExpectation::Disabled => assert!(matches!(
                reporter_discovery_status.activity,
                ReporterActivity::Disabled
            )),
            ReporterActivityExpectation::Idle => assert!(matches!(
                reporter_discovery_status.activity,
                ReporterActivity::Idle
            )),
            ReporterActivityExpectation::Queued => assert!(matches!(
                reporter_discovery_status.activity,
                ReporterActivity::Queued { .. }
            )),
            ReporterActivityExpectation::Running => assert!(matches!(
                reporter_discovery_status.activity,
                ReporterActivity::Running { .. }
            )),
        }
    }

    fn admit_without_polling_running_jobs(app: &mut App) {
        app.world_mut()
            .resource_scope::<Reporters, _>(|world, mut reporters| {
                let now = Instant::now();
                let discovery_limits = world.resource::<DiscoveryLimits>().clone();
                world.resource_scope::<DiscoveryControl, _>(|world, discovery_control| {
                    world.resource_scope::<DiscoveryStatus, _>(|world, mut discovery_status| {
                        reporters.admit(
                            world,
                            &discovery_control,
                            &discovery_limits,
                            &mut discovery_status,
                        );
                        reporters.refresh_activity(now, &mut discovery_status);
                    });
                });
            });
    }

    fn accept_finished_job_without_polling_other_jobs(app: &mut App, reporter_index: usize) {
        app.world_mut()
            .resource_scope::<Reporters, _>(|world, mut reporters| {
                let now = Instant::now();
                reporters.entries[reporter_index].drain_progress();
                reporters.entries[reporter_index].poll(now);
                let discovery_limits = world.resource::<DiscoveryLimits>().clone();
                world.resource_scope::<DiscoveryControl, _>(|world, discovery_control| {
                    world.resource_scope::<DiscoveryStatus, _>(|world, mut discovery_status| {
                        reporters.accept_completed(
                            &discovery_control,
                            &discovery_limits,
                            &mut discovery_status,
                        );
                        reporters.refresh_startup(&mut discovery_status);
                        reporters.admit(
                            world,
                            &discovery_control,
                            &discovery_limits,
                            &mut discovery_status,
                        );
                        reporters.refresh_activity(now, &mut discovery_status);
                    });
                });
            });
    }

    fn refresh_progress_without_polling_running_jobs(app: &mut App) {
        app.world_mut()
            .resource_scope::<Reporters, _>(|world, mut reporters| {
                let now = Instant::now();
                for reporter_entry in &mut reporters.entries {
                    reporter_entry.drain_progress();
                }
                let mut discovery_status = world.resource_mut::<DiscoveryStatus>();
                reporters.refresh_activity(now, &mut discovery_status);
            });
    }

    fn available_device_set(
        reporter_device_set_state: ReporterDeviceSetState<'_>,
    ) -> Result<&DeviceSet, DeviceSetUnavailable> {
        match reporter_device_set_state {
            ReporterDeviceSetState::NotRegistered => {
                Err(DeviceSetUnavailable::ReporterNotRegistered)
            },
            ReporterDeviceSetState::AwaitingCompleteSet => {
                Err(DeviceSetUnavailable::AwaitingCompleteSet)
            },
            ReporterDeviceSetState::Available(device_set) => Ok(device_set),
        }
    }

    fn initialize_io_task_pool() {
        IoTaskPool::get_or_init(|| TaskPoolBuilder::new().num_threads(4).build());
    }

    fn add_failed_required_reporter(app: &mut App) -> ReporterId {
        app.add_device_reporter(
            SequenceReporter {
                scans: std::collections::VecDeque::from([DeviceScan::Failed(
                    DeviceAccessError::Transport {
                        detail: String::from("test required discovery failure"),
                    },
                )]),
            },
            ReporterRegistration::required(
                DiscoveryCadence::OnDemand,
                ReporterCoverage::MatchingEvidenceOnly,
            ),
        )
    }

    fn add_incomplete_required_reporter(
        app: &mut App,
        background_job_gate: Arc<BackgroundJobGate>,
    ) -> ReporterId {
        app.add_device_reporter(
            GatedBackgroundReporter {
                background_job_gate,
            },
            ReporterRegistration::required(
                DiscoveryCadence::OnDemand,
                ReporterCoverage::MatchingEvidenceOnly,
            ),
        )
    }

    fn assert_required_failure_outweighs_incomplete_reporter(
        registration_order: RequiredReporterRegistrationOrder,
    ) {
        initialize_io_task_pool();
        let mut app = App::new();
        app.add_plugins(RiggingPlugin);
        let (started_sender, started_receiver) = channel();
        let (background_job_gate, background_job_release) = background_job_gate(
            "incomplete",
            started_sender,
            DiscoveryProgress::Indeterminate,
        );
        let (failed, incomplete) = match registration_order {
            RequiredReporterRegistrationOrder::FailureBeforeIncomplete => {
                let failed = add_failed_required_reporter(&mut app);
                let incomplete = add_incomplete_required_reporter(&mut app, background_job_gate);
                (failed, incomplete)
            },
            RequiredReporterRegistrationOrder::IncompleteBeforeFailure => {
                let incomplete = add_incomplete_required_reporter(&mut app, background_job_gate);
                let failed = add_failed_required_reporter(&mut app);
                (failed, incomplete)
            },
        };

        app.update();
        assert_eq!(
            wait_for_started_jobs(&started_receiver, 1),
            vec!["incomplete"]
        );
        app.update();

        let discovery_status = app.world().resource::<DiscoveryStatus>();
        assert!(matches!(
            discovery_status
                .reporter_status(failed)
                .expect("registered reporter must retain status"),
            crate::ReporterDiscoveryStatus {
                last_outcome: LastDiscoveryOutcome::Failed { .. },
                completed_batches: 1,
                ..
            }
        ));
        assert!(matches!(
            discovery_status
                .reporter_status(incomplete)
                .expect("registered reporter must retain status"),
            crate::ReporterDiscoveryStatus {
                last_outcome: LastDiscoveryOutcome::NotCompleted,
                completed_batches: 0,
                ..
            }
        ));
        assert!(matches!(
            discovery_status.startup,
            StartupDiscoveryState::BlockedByFailure { reporter, .. } if reporter == failed
        ));

        background_job_release.release();
    }

    fn finish_background_discovery_task(reporter_entry: &mut ReporterEntry, now: Instant) {
        const MAX_POLLS: usize = 10_000;

        for _ in 0..MAX_POLLS {
            reporter_entry.poll(now);
            if matches!(
                reporter_entry.completion_time(),
                ReporterCompletionTime::CompletedAt(_)
            ) {
                return;
            }
            std::thread::yield_now();
        }

        assert!(matches!(
            reporter_entry.completion_time(),
            ReporterCompletionTime::CompletedAt(_)
        ));
    }

    fn expire_reporter_deadline(app: &mut App, reporter: ReporterId) {
        let mut reporters = app.world_mut().resource_mut::<Reporters>();
        let reporter_entry = reporters
            .entries
            .iter_mut()
            .find(|reporter_entry| reporter_entry.reporter_id == reporter)
            .expect("registered reporter must retain its scheduler entry");
        reporter_entry.next_due = NextDue::At(Instant::now());
    }

    fn assert_expired_deadline_supplies_one_background_run(cadence: DiscoveryCadence) {
        initialize_io_task_pool();
        let discoveries = Arc::new(AtomicUsize::new(0));
        let mut app = App::new();
        app.add_plugins(RiggingPlugin);
        let (started_sender, started_receiver) = channel();
        let (background_job_gate, background_job_release) =
            background_job_gate("subject", started_sender, DiscoveryProgress::Indeterminate);
        let reporter = app.add_device_reporter(
            OutcomeThenGatedReporter {
                first_discovery_outcome: FirstDiscoveryOutcome::Succeeded,
                discoveries: Arc::clone(&discoveries),
                runs: 0,
                background_job_gate,
            },
            ReporterRegistration::required(cadence, ReporterCoverage::MatchingEvidenceOnly),
        );

        app.update();
        app.update();
        assert_eq!(discoveries.load(Ordering::Relaxed), 1);
        assert_eq!(
            app.world()
                .resource::<DiscoveryStatus>()
                .reporter_status(reporter)
                .expect("registered reporter must retain status")
                .completed_batches,
            1
        );

        expire_reporter_deadline(&mut app, reporter);
        app.update();
        assert_eq!(wait_for_started_jobs(&started_receiver, 1), vec!["subject"]);
        assert_eq!(discoveries.load(Ordering::Relaxed), 2);

        for _ in 0..3 {
            app.update();
            assert!(matches!(
                app.world()
                    .resource::<DiscoveryStatus>()
                    .reporter_status(reporter)
                    .expect("registered reporter must retain status")
                    .activity,
                ReporterActivity::Running { .. }
            ));
        }
        assert_eq!(discoveries.load(Ordering::Relaxed), 2);

        background_job_release.release();
        update_until_completed_batches(&mut app, reporter, 2);
        for _ in 0..3 {
            app.update();
        }

        let reporter_discovery_status = app
            .world()
            .resource::<DiscoveryStatus>()
            .reporter_status(reporter)
            .expect("registered reporter must retain status");
        assert_eq!(discoveries.load(Ordering::Relaxed), 2);
        assert_eq!(reporter_discovery_status.completed_batches, 2);
        assert!(matches!(
            reporter_discovery_status.activity,
            ReporterActivity::Idle
        ));
        assert_no_additional_job_started(&started_receiver);
    }

    impl DeviceReporter for SequenceReporter {
        fn discover(&mut self) -> DiscoveryWork {
            let device_scan = self
                .scans
                .pop_front()
                .unwrap_or(DeviceScan::Complete(Vec::new()));
            DiscoveryWork::Immediate(MainThreadDiscoveryJob::new(move |_| device_scan))
        }
    }

    fn test_device_record() -> DeviceRecord {
        DeviceRecord {
            reported_as:  ReportedAs::MatchEvidenceOnly,
            transport:    None,
            presence:     Presence::Present,
            claim:        Claim::NotApplicable,
            capabilities: Capabilities::new(),
            serial:       ReportedSerial::NotExposedByUnit,
            os_id:        OsDeviceId::PlatformReportedNothing,
            attachment:   AttachmentPath::PlatformHasNoConcept,
            descriptor:   DeviceDescriptor::PlatformReportedNothing,
        }
    }

    impl DeviceReporter for RecordReporter {
        fn discover(&mut self) -> DiscoveryWork {
            self.scans.fetch_add(1, Ordering::Relaxed);
            DiscoveryWork::Immediate(MainThreadDiscoveryJob::new(|_| {
                DeviceScan::Complete(vec![test_device_record()])
            }))
        }
    }

    #[derive(Component, Reflect)]
    #[reflect(Component)]
    struct TestConfiguration;

    struct TestDriver;

    impl EndpointDriver for TestDriver {
        type Configuration = TestConfiguration;

        fn capture(
            &mut self,
            _: &mut World,
            _: &DeviceEndpoint,
        ) -> CaptureOutcome<Self::Configuration> {
            CaptureOutcome::ReadFailed(DeviceAccessError::Absent {
                detail: String::from("test driver has no endpoint"),
            })
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
            AttemptProgress::Pending
        }
    }

    struct FirstSchemePlugin(SchemeName);

    impl Plugin for FirstSchemePlugin {
        fn build(&self, app: &mut App) { app.register_device_scheme(self.0.clone()); }
    }

    struct SecondSchemePlugin(SchemeName);

    impl Plugin for SecondSchemePlugin {
        fn build(&self, app: &mut App) { app.register_device_scheme(self.0.clone()); }
    }

    #[test]
    fn reporter_completion_types_name_absent_ready_and_accepted_transitions() {
        let started_at = Instant::now();
        let completed_at = started_at + Duration::from_secs(1);
        let mut reporter_entry = ReporterEntry::new(
            CountingReporter {
                scans: Arc::new(AtomicUsize::new(0)),
            },
            ReporterId(0),
            ReporterRegistration::required(
                DiscoveryCadence::OnDemand,
                ReporterCoverage::MatchingEvidenceOnly,
            ),
        );

        assert!(matches!(
            reporter_entry.completion_time(),
            ReporterCompletionTime::NoCompletedResult
        ));
        assert!(matches!(
            reporter_entry.take_completed(ReporterActivation::Enabled),
            ReporterCompletionAcceptance::NoCompletedResult
        ));
        assert!(matches!(
            reporter_entry.state,
            ReporterRunState::Idle {
                rerun: RerunRequest::NotRequested,
            }
        ));

        reporter_entry.state = ReporterRunState::Queued {
            batch:     DiscoveryBatchId(7),
            queued_at: started_at,
            rerun:     RerunRequest::Requested,
            pending:   PendingDiscovery::Completed {
                scan: DeviceScan::Complete(Vec::new()),
                completed_at,
            },
        };
        assert_eq!(
            reporter_entry.completion_time(),
            ReporterCompletionTime::CompletedAt(completed_at)
        );

        let completion_acceptance = reporter_entry.take_completed(ReporterActivation::Enabled);
        assert!(matches!(
            &completion_acceptance,
            ReporterCompletionAcceptance::Accepted(_)
        ));
        let ReporterCompletionAcceptance::Accepted(completed_discovery) = completion_acceptance
        else {
            return;
        };
        assert_eq!(completed_discovery.batch, DiscoveryBatchId(7));
        assert_eq!(completed_discovery.started_at, started_at);
        assert_eq!(completed_discovery.completed_at, completed_at);
        assert!(matches!(
            completed_discovery.scan,
            DeviceScan::Complete(devices) if devices.is_empty()
        ));
        assert!(matches!(
            reporter_entry.completion_time(),
            ReporterCompletionTime::NoCompletedResult
        ));
        assert!(matches!(
            reporter_entry.state,
            ReporterRunState::Idle {
                rerun: RerunRequest::Requested,
            }
        ));
        assert!(matches!(
            reporter_entry.take_completed(ReporterActivation::Enabled),
            ReporterCompletionAcceptance::NoCompletedResult
        ));
    }

    #[test]
    fn periodic_reporter_runs_again_after_accepting_its_previous_whole_set() {
        let scans = Arc::new(AtomicUsize::new(0));
        let mut app = App::new();
        app.add_plugins(RiggingPlugin);
        let _: ReporterId = app.add_device_reporter(
            CountingReporter {
                scans: Arc::clone(&scans),
            },
            ReporterRegistration::required(
                DiscoveryCadence::Periodic {
                    interval: Duration::ZERO,
                },
                ReporterCoverage::MatchingEvidenceOnly,
            ),
        );

        app.update();
        assert_eq!(scans.load(Ordering::Relaxed), 1);

        app.update();
        assert_eq!(scans.load(Ordering::Relaxed), 2);
    }

    #[test]
    fn periodic_background_deadline_supplies_one_run_across_multiple_updates() {
        assert_expired_deadline_supplies_one_background_run(DiscoveryCadence::Periodic {
            interval: Duration::from_secs(30),
        });
    }

    #[test]
    fn event_driven_background_backstop_supplies_one_run_across_multiple_updates() {
        assert_expired_deadline_supplies_one_background_run(DiscoveryCadence::EventDriven {
            backstop: Duration::from_secs(30),
        });
    }

    #[test]
    fn periodic_deadline_uses_completion_without_catch_up_runs() {
        let scans = Arc::new(AtomicUsize::new(0));
        let interval = Duration::from_secs(10);
        let initial_due = Instant::now();
        let first_completed_at = initial_due + Duration::from_secs(35);
        let mut reporter_entry = ReporterEntry::new(
            CountingReporter {
                scans: Arc::clone(&scans),
            },
            ReporterId(0),
            ReporterRegistration::required(
                DiscoveryCadence::Periodic { interval },
                ReporterCoverage::MatchingEvidenceOnly,
            ),
        );

        assert!(reporter_entry.record_due_signal(
            initial_due,
            DiscoveryRequest::Requested,
            DiscoveryDirtyState::Clean,
        ));
        reporter_entry.queue(DiscoveryBatchId(0), initial_due);
        reporter_entry.prepare(&mut World::new());
        assert!(matches!(
            reporter_entry.take_completed(ReporterActivation::Enabled),
            ReporterCompletionAcceptance::Accepted(_)
        ));
        reporter_entry.schedule_after_completion(first_completed_at);
        assert!(matches!(
            reporter_entry.next_due,
            NextDue::At(deadline) if deadline == first_completed_at + interval
        ));
        assert!(!reporter_entry.record_due_signal(
            first_completed_at + interval.saturating_sub(Duration::from_nanos(1)),
            DiscoveryRequest::NotRequested,
            DiscoveryDirtyState::Clean,
        ));

        let much_later = first_completed_at + interval * 4;
        assert!(reporter_entry.record_due_signal(
            much_later,
            DiscoveryRequest::NotRequested,
            DiscoveryDirtyState::Clean,
        ));
        reporter_entry.queue(DiscoveryBatchId(1), much_later);
        for _ in 0..3 {
            assert!(!reporter_entry.record_due_signal(
                much_later,
                DiscoveryRequest::NotRequested,
                DiscoveryDirtyState::Clean,
            ));
        }
        reporter_entry.prepare(&mut World::new());
        let second_completed_at = much_later + Duration::from_secs(1);
        assert!(matches!(
            reporter_entry.take_completed(ReporterActivation::Enabled),
            ReporterCompletionAcceptance::Accepted(_)
        ));
        reporter_entry.schedule_after_completion(second_completed_at);

        assert_eq!(scans.load(Ordering::Relaxed), 2);
        assert!(matches!(
            reporter_entry.next_due,
            NextDue::At(deadline) if deadline == second_completed_at + interval
        ));
        assert!(!reporter_entry.record_due_signal(
            second_completed_at,
            DiscoveryRequest::NotRequested,
            DiscoveryDirtyState::Clean,
        ));
    }

    #[test]
    fn event_driven_notifications_coalesce_and_backstop_runs_once() {
        let scans = Arc::new(AtomicUsize::new(0));
        let backstop = Duration::from_secs(30);
        let first_completed_at = Instant::now();
        let mut reporter_entry = ReporterEntry::new(
            CountingReporter {
                scans: Arc::clone(&scans),
            },
            ReporterId(0),
            ReporterRegistration::required(
                DiscoveryCadence::EventDriven { backstop },
                ReporterCoverage::MatchingEvidenceOnly,
            ),
        );

        reporter_entry.schedule_after_completion(first_completed_at);
        let dirty_at = first_completed_at + Duration::from_secs(1);
        assert!(reporter_entry.record_due_signal(
            dirty_at,
            DiscoveryRequest::NotRequested,
            DiscoveryDirtyState::Dirty,
        ));
        reporter_entry.queue(DiscoveryBatchId(0), dirty_at);
        for _ in 0..3 {
            assert!(!reporter_entry.record_due_signal(
                dirty_at,
                DiscoveryRequest::NotRequested,
                DiscoveryDirtyState::Dirty,
            ));
        }
        reporter_entry.prepare(&mut World::new());
        assert!(matches!(
            reporter_entry.take_completed(ReporterActivation::Enabled),
            ReporterCompletionAcceptance::Accepted(_)
        ));
        reporter_entry.schedule_after_completion(dirty_at);
        assert_eq!(scans.load(Ordering::Relaxed), 1);

        assert!(!reporter_entry.record_due_signal(
            dirty_at + backstop.saturating_sub(Duration::from_nanos(1)),
            DiscoveryRequest::NotRequested,
            DiscoveryDirtyState::Clean,
        ));
        let backstop_due = dirty_at + backstop;
        assert!(reporter_entry.record_due_signal(
            backstop_due,
            DiscoveryRequest::NotRequested,
            DiscoveryDirtyState::Clean,
        ));
        reporter_entry.queue(DiscoveryBatchId(1), backstop_due);
        for _ in 0..3 {
            assert!(!reporter_entry.record_due_signal(
                backstop_due + backstop,
                DiscoveryRequest::NotRequested,
                DiscoveryDirtyState::Clean,
            ));
        }
        reporter_entry.prepare(&mut World::new());

        assert_eq!(scans.load(Ordering::Relaxed), 2);
    }

    #[test]
    fn two_due_reporters_prepare_whole_sets_in_one_update() {
        let first_scans = Arc::new(AtomicUsize::new(0));
        let second_scans = Arc::new(AtomicUsize::new(0));
        let mut app = App::new();
        app.add_plugins(RiggingPlugin);
        app.add_device_reporter(
            RecordReporter {
                scans: Arc::clone(&first_scans),
            },
            ReporterRegistration::required(
                DiscoveryCadence::OnDemand,
                ReporterCoverage::MatchingEvidenceOnly,
            ),
        );
        app.add_device_reporter(
            CountingReporter {
                scans: Arc::clone(&second_scans),
            },
            ReporterRegistration::required(
                DiscoveryCadence::OnDemand,
                ReporterCoverage::MatchingEvidenceOnly,
            ),
        );

        app.update();

        assert_eq!(first_scans.load(Ordering::Relaxed), 1);
        assert_eq!(second_scans.load(Ordering::Relaxed), 1);
    }

    #[test]
    fn default_limit_holds_two_io_jobs_in_flight_and_queues_a_third() {
        initialize_io_task_pool();
        assert_eq!(IoTaskPool::get().thread_num(), 4);
        let mut app = App::new();
        app.add_plugins(RiggingPlugin);
        let (started_sender, started_receiver) = channel();
        let (first_gate, first_release) = background_job_gate(
            "first",
            started_sender.clone(),
            DiscoveryProgress::Indeterminate,
        );
        let (second_gate, second_release) = background_job_gate(
            "second",
            started_sender.clone(),
            DiscoveryProgress::Indeterminate,
        );
        let (third_gate, third_release) =
            background_job_gate("third", started_sender, DiscoveryProgress::Indeterminate);
        let first = app.add_device_reporter(
            GatedBackgroundReporter {
                background_job_gate: first_gate,
            },
            ReporterRegistration::required(
                DiscoveryCadence::OnDemand,
                ReporterCoverage::MatchingEvidenceOnly,
            ),
        );
        let second = app.add_device_reporter(
            GatedBackgroundReporter {
                background_job_gate: second_gate,
            },
            ReporterRegistration::required(
                DiscoveryCadence::OnDemand,
                ReporterCoverage::MatchingEvidenceOnly,
            ),
        );
        let third = app.add_device_reporter(
            GatedBackgroundReporter {
                background_job_gate: third_gate,
            },
            ReporterRegistration::required(
                DiscoveryCadence::OnDemand,
                ReporterCoverage::MatchingEvidenceOnly,
            ),
        );

        let release_jobs =
            release_jobs_after_start(started_receiver, 2, vec![first_release, second_release]);
        app.update();

        let (reporter_names, started_receiver) = release_jobs
            .join()
            .expect("gate coordinator must return observed reporter names");
        assert_eq!(reporter_names, vec!["first", "second"]);
        assert_no_additional_job_started(&started_receiver);
        let discovery_status = app.world().resource::<DiscoveryStatus>();
        assert!(matches!(
            discovery_status
                .reporter_status(first)
                .expect("registered reporter must retain status")
                .activity,
            ReporterActivity::Running { .. }
        ));
        assert!(matches!(
            discovery_status
                .reporter_status(second)
                .expect("registered reporter must retain status")
                .activity,
            ReporterActivity::Running { .. }
        ));
        assert!(matches!(
            discovery_status
                .reporter_status(third)
                .expect("registered reporter must retain status")
                .activity,
            ReporterActivity::Queued {
                batch: DiscoveryBatchId(0),
            }
        ));

        third_release.release();
        update_until_completed_batches(&mut app, first, 1);
        update_until_completed_batches(&mut app, second, 1);
        update_until_completed_batches(&mut app, third, 1);
    }

    #[test]
    fn prepared_background_runtime_starts_when_capacity_admits_it() {
        initialize_io_task_pool();
        let mut app = App::new();
        app.add_plugins(RiggingPlugin);
        let progress_after = Duration::from_mins(1);
        {
            let mut discovery_limits = app.world_mut().resource_mut::<DiscoveryLimits>();
            discovery_limits.set_max_concurrent_jobs(NonZeroUsize::MIN);
            discovery_limits.set_progress_after(progress_after);
        }
        let (started_sender, started_receiver) = channel();
        let (blocker_gate, blocker_release) = background_job_gate(
            "blocker",
            started_sender.clone(),
            DiscoveryProgress::Indeterminate,
        );
        let (subject_gate, subject_release) =
            background_job_gate("subject", started_sender, DiscoveryProgress::Indeterminate);
        let blocker = app.add_device_reporter(
            GatedBackgroundReporter {
                background_job_gate: blocker_gate,
            },
            ReporterRegistration::required(
                DiscoveryCadence::OnDemand,
                ReporterCoverage::MatchingEvidenceOnly,
            ),
        );
        let subject = app.add_device_reporter(
            GatedBackgroundReporter {
                background_job_gate: subject_gate,
            },
            ReporterRegistration::required(
                DiscoveryCadence::OnDemand,
                ReporterCoverage::MatchingEvidenceOnly,
            ),
        );

        app.update();
        assert_eq!(wait_for_started_jobs(&started_receiver, 1), vec!["blocker"]);
        assert_no_additional_job_started(&started_receiver);
        {
            let mut reporters = app.world_mut().resource_mut::<Reporters>();
            let subject_entry = reporters
                .entries
                .iter_mut()
                .find(|reporter_entry| reporter_entry.reporter_id == subject)
                .expect("registered reporter must retain its scheduler entry");
            assert!(subject_entry.has_prepared_background());
            if let ReporterRunState::Queued { queued_at, .. } = &mut subject_entry.state {
                *queued_at = Instant::now()
                    .checked_sub(progress_after * 2)
                    .expect("test duration must fit before the current instant");
            }
        }

        app.world_mut()
            .resource_mut::<DiscoveryLimits>()
            .set_max_concurrent_jobs(NonZeroUsize::new(2).unwrap_or(NonZeroUsize::MIN));
        admit_without_polling_running_jobs(&mut app);
        assert_eq!(wait_for_started_jobs(&started_receiver, 1), vec!["subject"]);

        assert!(matches!(
            app.world()
                .resource::<DiscoveryStatus>()
                .reporter_status(subject)
                .expect("registered reporter must retain status")
                .activity,
            ReporterActivity::Running { elapsed, .. } if elapsed < progress_after
        ));

        subject_release.release();
        blocker_release.release();
        update_until_completed_batches(&mut app, subject, 1);
        update_until_completed_batches(&mut app, blocker, 1);
        assert!(matches!(
            app.world()
                .resource::<DiscoveryStatus>()
                .reporter_status(subject)
                .expect("registered reporter must retain status")
                .last_outcome,
            LastDiscoveryOutcome::Succeeded { duration, .. } if duration < progress_after
        ));
    }

    #[test]
    fn raising_runtime_limit_admits_waiting_job_without_cancelling_running_job() {
        initialize_io_task_pool();
        let mut app = App::new();
        app.add_plugins(RiggingPlugin);
        app.world_mut()
            .resource_mut::<DiscoveryLimits>()
            .set_max_concurrent_jobs(NonZeroUsize::MIN);
        let (started_sender, started_receiver) = channel();
        let (first_gate, first_release) = background_job_gate(
            "first",
            started_sender.clone(),
            DiscoveryProgress::Indeterminate,
        );
        let (second_gate, second_release) =
            background_job_gate("second", started_sender, DiscoveryProgress::Indeterminate);
        let first = app.add_device_reporter(
            GatedBackgroundReporter {
                background_job_gate: first_gate,
            },
            ReporterRegistration::required(
                DiscoveryCadence::OnDemand,
                ReporterCoverage::MatchingEvidenceOnly,
            ),
        );
        let second = app.add_device_reporter(
            GatedBackgroundReporter {
                background_job_gate: second_gate,
            },
            ReporterRegistration::required(
                DiscoveryCadence::OnDemand,
                ReporterCoverage::MatchingEvidenceOnly,
            ),
        );

        let release_first_job = release_jobs_after_start(started_receiver, 1, vec![first_release]);
        app.update();
        let (reporter_names, started_receiver) = release_first_job
            .join()
            .expect("gate coordinator must return the first reporter name");
        assert_eq!(reporter_names, vec!["first"]);
        assert_no_additional_job_started(&started_receiver);
        assert!(matches!(
            app.world()
                .resource::<DiscoveryStatus>()
                .reporter_status(second)
                .expect("registered reporter must retain status")
                .activity,
            ReporterActivity::Queued { .. }
        ));

        app.world_mut()
            .resource_mut::<DiscoveryLimits>()
            .set_max_concurrent_jobs(NonZeroUsize::new(2).unwrap_or(NonZeroUsize::MIN));
        let release_second_job =
            release_jobs_after_start(started_receiver, 1, vec![second_release]);
        admit_without_polling_running_jobs(&mut app);

        let (reporter_names, _) = release_second_job
            .join()
            .expect("gate coordinator must return the second reporter name");
        assert_eq!(reporter_names, vec!["second"]);
        let discovery_status = app.world().resource::<DiscoveryStatus>();
        assert!(matches!(
            discovery_status
                .reporter_status(first)
                .expect("registered reporter must retain status")
                .activity,
            ReporterActivity::Running { .. }
        ));
        assert!(matches!(
            discovery_status
                .reporter_status(second)
                .expect("registered reporter must retain status")
                .activity,
            ReporterActivity::Running { .. }
        ));

        update_until_completed_batches(&mut app, first, 1);
        update_until_completed_batches(&mut app, second, 1);
    }

    #[test]
    fn lowering_runtime_limit_changes_later_admission_without_cancelling_running_jobs() {
        initialize_io_task_pool();
        let mut app = App::new();
        app.add_plugins(RiggingPlugin);
        let (started_sender, started_receiver) = channel();
        let (first_gate, first_release) = background_job_gate(
            "first",
            started_sender.clone(),
            DiscoveryProgress::Indeterminate,
        );
        let (second_gate, second_release) = background_job_gate(
            "second",
            started_sender.clone(),
            DiscoveryProgress::Indeterminate,
        );
        let (third_gate, third_release) =
            background_job_gate("third", started_sender, DiscoveryProgress::Indeterminate);
        let first = app.add_device_reporter(
            GatedBackgroundReporter {
                background_job_gate: first_gate,
            },
            ReporterRegistration::required(
                DiscoveryCadence::OnDemand,
                ReporterCoverage::MatchingEvidenceOnly,
            ),
        );
        let second = app.add_device_reporter(
            GatedBackgroundReporter {
                background_job_gate: second_gate,
            },
            ReporterRegistration::required(
                DiscoveryCadence::OnDemand,
                ReporterCoverage::MatchingEvidenceOnly,
            ),
        );
        let third = app.add_device_reporter(
            GatedBackgroundReporter {
                background_job_gate: third_gate,
            },
            ReporterRegistration::required(
                DiscoveryCadence::OnDemand,
                ReporterCoverage::MatchingEvidenceOnly,
            ),
        );

        let release_jobs =
            release_jobs_after_start(started_receiver, 2, vec![first_release, second_release]);
        app.update();
        let (reporter_names, started_receiver) = release_jobs
            .join()
            .expect("gate coordinator must return observed reporter names");
        assert_eq!(reporter_names, vec!["first", "second"]);
        app.world_mut()
            .resource_mut::<DiscoveryLimits>()
            .set_max_concurrent_jobs(NonZeroUsize::MIN);
        {
            let discovery_status = app.world().resource::<DiscoveryStatus>();
            assert!(matches!(
                discovery_status
                    .reporter_status(first)
                    .expect("registered reporter must retain status")
                    .activity,
                ReporterActivity::Running { .. }
            ));
            assert!(matches!(
                discovery_status
                    .reporter_status(second)
                    .expect("registered reporter must retain status")
                    .activity,
                ReporterActivity::Running { .. }
            ));
        }

        accept_finished_job_without_polling_other_jobs(&mut app, 0);

        assert_no_additional_job_started(&started_receiver);
        let discovery_status = app.world().resource::<DiscoveryStatus>();
        assert!(matches!(
            discovery_status
                .reporter_status(second)
                .expect("registered reporter must retain status")
                .activity,
            ReporterActivity::Running { .. }
        ));
        assert!(matches!(
            discovery_status
                .reporter_status(third)
                .expect("registered reporter must retain status")
                .activity,
            ReporterActivity::Queued { .. }
        ));

        third_release.release();
        update_until_completed_batches(&mut app, second, 1);
        update_until_completed_batches(&mut app, third, 1);
    }

    #[test]
    fn required_and_optional_reporters_receive_distinct_startup_batches() {
        let discoveries = Arc::new(Mutex::new(Vec::new()));
        let mut app = App::new();
        app.add_plugins(RiggingPlugin);
        let required = app.add_device_reporter(
            OrderedReporter {
                name:        "required",
                discoveries: Arc::clone(&discoveries),
            },
            ReporterRegistration::required(
                DiscoveryCadence::OnDemand,
                ReporterCoverage::MatchingEvidenceOnly,
            ),
        );
        let optional = app.add_device_reporter(
            OrderedReporter {
                name:        "optional",
                discoveries: Arc::clone(&discoveries),
            },
            ReporterRegistration::optional(
                DiscoveryCadence::OnDemand,
                ReporterActivation::Enabled,
                ReporterCoverage::MatchingEvidenceOnly,
            ),
        );

        app.update();
        assert_eq!(
            discoveries
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .as_slice(),
            ["required"]
        );
        assert!(matches!(
            app.world().resource::<DiscoveryStatus>().startup,
            StartupDiscoveryState::Discovering
        ));

        app.update();
        assert_eq!(
            discoveries
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .as_slice(),
            ["required", "optional"]
        );
        assert!(matches!(
            app.world().resource::<DiscoveryStatus>().startup,
            StartupDiscoveryState::Ready
        ));

        app.update();
        let discovery_status = app.world().resource::<DiscoveryStatus>();
        let required_outcome = &discovery_status
            .reporter_status(required)
            .expect("registered reporter must retain status")
            .last_outcome;
        let optional_outcome = &discovery_status
            .reporter_status(optional)
            .expect("registered reporter must retain status")
            .last_outcome;
        assert!(matches!(
            required_outcome,
            LastDiscoveryOutcome::Succeeded { .. }
        ));
        assert!(matches!(
            optional_outcome,
            LastDiscoveryOutcome::Succeeded { .. }
        ));
        let LastDiscoveryOutcome::Succeeded {
            batch: required_batch,
            ..
        } = required_outcome
        else {
            return;
        };
        let LastDiscoveryOutcome::Succeeded {
            batch: optional_batch,
            ..
        } = optional_outcome
        else {
            return;
        };
        assert_ne!(required_batch, optional_batch);
    }

    #[test]
    fn earlier_required_failure_outweighs_later_incomplete_reporter() {
        assert_required_failure_outweighs_incomplete_reporter(
            RequiredReporterRegistrationOrder::FailureBeforeIncomplete,
        );
    }

    #[test]
    fn later_required_failure_outweighs_earlier_incomplete_reporter() {
        assert_required_failure_outweighs_incomplete_reporter(
            RequiredReporterRegistrationOrder::IncompleteBeforeFailure,
        );
    }

    #[test]
    fn required_failure_retains_its_previous_set_and_blocks_readiness() {
        let mut app = App::new();
        app.add_plugins(RiggingPlugin);
        let reporter_id = app.add_device_reporter(
            SequenceReporter {
                scans: std::collections::VecDeque::from([
                    DeviceScan::Complete(Vec::new()),
                    DeviceScan::Failed(DeviceAccessError::Transport {
                        detail: String::from("test transport failure"),
                    }),
                ]),
            },
            ReporterRegistration::required(
                DiscoveryCadence::Periodic {
                    interval: Duration::ZERO,
                },
                ReporterCoverage::MatchingEvidenceOnly,
            ),
        );

        app.update();
        app.update();
        app.update();

        let device_set = available_device_set(
            app.world()
                .resource::<Reporters>()
                .latest_device_set(reporter_id),
        )
        .expect("failed discovery must retain the preceding whole set");
        assert_eq!(device_set.revision.get(), 1);
        assert!(matches!(
            app.world().resource::<DiscoveryStatus>().startup,
            StartupDiscoveryState::BlockedByFailure { reporter, .. } if reporter == reporter_id
        ));
    }

    #[test]
    fn reconciliation_handoff_retains_changed_sets_and_failures_until_phase_nine_drains_them() {
        let mut app = App::new();
        app.add_plugins(RiggingPlugin);
        let reporter_id = app.add_device_reporter(
            SequenceReporter {
                scans: std::collections::VecDeque::from([
                    DeviceScan::Complete(Vec::new()),
                    DeviceScan::Failed(DeviceAccessError::Transport {
                        detail: String::from("test reconciliation handoff"),
                    }),
                ]),
            },
            ReporterRegistration::required(
                DiscoveryCadence::Periodic {
                    interval: Duration::ZERO,
                },
                ReporterCoverage::MatchingEvidenceOnly,
            ),
        );

        app.update();
        app.update();
        app.update();

        let mut reporters = app.world_mut().resource_mut::<Reporters>();
        assert_eq!(reporters.take_changed_reporters(), vec![reporter_id]);
        let reporter_failures = reporters.take_reporter_failures();
        assert_eq!(reporter_failures.len(), 1);
        assert_eq!(reporter_failures[0].reporter, reporter_id);
        assert!(matches!(
            reporter_failures[0].error,
            DeviceAccessError::Transport { .. }
        ));
    }

    #[test]
    fn enabling_optional_reporter_requests_initial_discovery_without_authored_inventory() {
        let scans = Arc::new(AtomicUsize::new(0));
        let mut app = App::new();
        app.add_plugins(RiggingPlugin);
        let reporter_id = app.add_device_reporter(
            CountingReporter {
                scans: Arc::clone(&scans),
            },
            ReporterRegistration::optional(
                DiscoveryCadence::OnDemand,
                ReporterActivation::Disabled,
                ReporterCoverage::MatchingEvidenceOnly,
            ),
        );

        app.update();
        assert_eq!(scans.load(Ordering::Relaxed), 0);
        app.world_mut()
            .resource_mut::<DiscoveryControl>()
            .enable(reporter_id)
            .expect("registered optional reporter must enable");
        app.update();

        assert_eq!(scans.load(Ordering::Relaxed), 1);
    }

    #[test]
    fn completion_budget_accepts_whole_reporter_sets_without_splitting_them() {
        let second_scans = Arc::new(AtomicUsize::new(0));
        let mut app = App::new();
        app.add_plugins(RiggingPlugin);
        app.world_mut()
            .resource_mut::<DiscoveryLimits>()
            .set_max_completions_per_frame(std::num::NonZeroUsize::MIN);
        let first = app.add_device_reporter(
            SequenceReporter {
                scans: std::collections::VecDeque::from([DeviceScan::Complete(vec![
                    test_device_record(),
                    test_device_record(),
                    test_device_record(),
                ])]),
            },
            ReporterRegistration::required(
                DiscoveryCadence::OnDemand,
                ReporterCoverage::MatchingEvidenceOnly,
            ),
        );
        let second = app.add_device_reporter(
            CountingReporter {
                scans: Arc::clone(&second_scans),
            },
            ReporterRegistration::required(
                DiscoveryCadence::OnDemand,
                ReporterCoverage::MatchingEvidenceOnly,
            ),
        );

        app.update();
        app.update();

        let first_status = app
            .world()
            .resource::<DiscoveryStatus>()
            .reporter_status(first)
            .expect("registered reporter must retain status");
        let second_status = app
            .world()
            .resource::<DiscoveryStatus>()
            .reporter_status(second)
            .expect("registered reporter must retain status");
        assert!(matches!(
            first_status.last_outcome,
            LastDiscoveryOutcome::Succeeded { .. }
        ));
        assert!(matches!(
            second_status.last_outcome,
            LastDiscoveryOutcome::NotCompleted
        ));
        assert_eq!(second_scans.load(Ordering::Relaxed), 1);
        let first_device_set =
            available_device_set(app.world().resource::<Reporters>().latest_device_set(first))
                .expect("accepted complete set must remain available");
        assert_eq!(first_device_set.devices.len(), 3);
        assert_eq!(first_device_set.revision.get(), 1);

        app.update();
        assert!(matches!(
            app.world()
                .resource::<DiscoveryStatus>()
                .reporter_status(second)
                .expect("registered reporter must retain status")
                .last_outcome,
            LastDiscoveryOutcome::Succeeded { .. }
        ));
    }

    #[test]
    fn completion_budget_preserves_actual_completion_clock_for_delayed_acceptance() {
        let scans = Arc::new(AtomicUsize::new(0));
        let cadence_interval = Duration::from_secs(10);
        let first_registration = ReporterRegistration::required(
            DiscoveryCadence::OnDemand,
            ReporterCoverage::MatchingEvidenceOnly,
        );
        let delayed_registration = ReporterRegistration::required(
            DiscoveryCadence::Periodic {
                interval: cadence_interval,
            },
            ReporterCoverage::MatchingEvidenceOnly,
        );
        let mut reporters = Reporters::default();
        let first = reporters.add(
            CountingReporter {
                scans: Arc::clone(&scans),
            },
            first_registration.clone(),
        );
        let delayed = reporters.add(CountingReporter { scans }, delayed_registration.clone());
        let mut discovery_control = DiscoveryControl::default();
        discovery_control.register(first, &first_registration);
        discovery_control.register(delayed, &delayed_registration);
        let mut discovery_status = DiscoveryStatus::default();
        discovery_status.register(first, &first_registration);
        discovery_status.register(delayed, &delayed_registration);
        let mut discovery_limits = DiscoveryLimits::default();
        discovery_limits.set_max_completions_per_frame(NonZeroUsize::MIN);

        let started_at = Instant::now();
        let first_completed_at = started_at + Duration::from_secs(1);
        let delayed_completed_at = started_at + Duration::from_secs(2);
        let delayed_accepted_at = delayed_completed_at + Duration::from_secs(40);
        reporters.entries[0].state = ReporterRunState::Queued {
            batch:     DiscoveryBatchId(0),
            queued_at: started_at,
            rerun:     RerunRequest::NotRequested,
            pending:   PendingDiscovery::Completed {
                scan:         DeviceScan::Complete(Vec::new()),
                completed_at: first_completed_at,
            },
        };
        reporters.entries[1].state = ReporterRunState::Queued {
            batch:     DiscoveryBatchId(1),
            queued_at: started_at,
            rerun:     RerunRequest::NotRequested,
            pending:   PendingDiscovery::Completed {
                scan:         DeviceScan::Complete(Vec::new()),
                completed_at: delayed_completed_at,
            },
        };

        reporters.accept_completed(&discovery_control, &discovery_limits, &mut discovery_status);
        assert!(matches!(
            discovery_status
                .reporter_status(first)
                .expect("registered reporter must retain status")
                .last_outcome,
            LastDiscoveryOutcome::Succeeded { .. }
        ));
        assert!(matches!(
            discovery_status
                .reporter_status(delayed)
                .expect("registered reporter must retain status")
                .last_outcome,
            LastDiscoveryOutcome::NotCompleted
        ));

        reporters.accept_completed(&discovery_control, &discovery_limits, &mut discovery_status);

        let expected_duration = delayed_completed_at.duration_since(started_at);
        assert!(matches!(
            discovery_status
                .reporter_status(delayed)
                .expect("registered reporter must retain status")
                .last_outcome,
            LastDiscoveryOutcome::Succeeded { duration, .. } if duration == expected_duration
        ));
        assert_ne!(
            expected_duration,
            delayed_accepted_at.duration_since(started_at)
        );
        assert!(matches!(
            &reporters.entries[1].latest_set,
            RetainedDeviceSet::Complete { completed_at, .. }
                if *completed_at == delayed_completed_at
        ));
        assert!(matches!(
            &reporters.entries[1].next_due,
            NextDue::At(deadline) if *deadline == delayed_completed_at + cadence_interval
        ));
        assert!(reporters.entries[1].record_due_signal(
            delayed_accepted_at,
            DiscoveryRequest::NotRequested,
            DiscoveryDirtyState::Clean,
        ));
    }

    #[test]
    fn later_co_reporter_completion_preserves_unchanged_retained_set() {
        let mut app = App::new();
        app.add_plugins(RiggingPlugin);
        let first = app.add_device_reporter(
            SequenceReporter {
                scans: std::collections::VecDeque::from([DeviceScan::Complete(vec![
                    test_device_record(),
                    test_device_record(),
                ])]),
            },
            ReporterRegistration::required(
                DiscoveryCadence::OnDemand,
                ReporterCoverage::MatchingEvidenceOnly,
            ),
        );
        let second = app.add_device_reporter(
            SequenceReporter {
                scans: std::collections::VecDeque::from([DeviceScan::Complete(vec![
                    test_device_record(),
                ])]),
            },
            ReporterRegistration::optional(
                DiscoveryCadence::OnDemand,
                ReporterActivation::Disabled,
                ReporterCoverage::MatchingEvidenceOnly,
            ),
        );

        app.update();
        app.update();
        {
            let mut reporters = app.world_mut().resource_mut::<Reporters>();
            assert_eq!(reporters.take_changed_reporters(), vec![first]);
            let first_device_set = available_device_set(reporters.latest_device_set(first))
                .expect("first reporter's complete set must remain available");
            assert_eq!(first_device_set.devices.len(), 2);
            assert_eq!(first_device_set.revision.get(), 1);
            assert!(matches!(
                reporters.latest_device_set(second),
                ReporterDeviceSetState::AwaitingCompleteSet
            ));
        }

        app.world_mut()
            .resource_mut::<DiscoveryControl>()
            .enable(second)
            .expect("registered optional reporter must enable");
        app.update();
        app.update();

        let mut reporters = app.world_mut().resource_mut::<Reporters>();
        assert_eq!(reporters.take_changed_reporters(), vec![second]);
        let first_device_set = available_device_set(reporters.latest_device_set(first))
            .expect("unchanged first reporter set must stay borrowable");
        assert_eq!(first_device_set.devices.len(), 2);
        assert_eq!(first_device_set.revision.get(), 1);
        let second_device_set = available_device_set(reporters.latest_device_set(second))
            .expect("second reporter's later completion must be retained");
        assert_eq!(second_device_set.devices.len(), 1);
        assert_eq!(second_device_set.revision.get(), 1);
    }

    #[test]
    fn background_discovery_runs_on_io_pool_and_updates_its_retained_outcome() {
        const MAX_UPDATES: usize = 50;

        initialize_io_task_pool();
        let discoveries = Arc::new(AtomicUsize::new(0));
        let mut app = App::new();
        app.add_plugins(RiggingPlugin);
        let reporter_id = app.add_device_reporter(
            BackgroundReporter {
                discoveries: Arc::clone(&discoveries),
            },
            ReporterRegistration::required(
                DiscoveryCadence::OnDemand,
                ReporterCoverage::MatchingEvidenceOnly,
            ),
        );

        for _ in 0..MAX_UPDATES {
            app.update();
            if matches!(
                app.world()
                    .resource::<DiscoveryStatus>()
                    .reporter_status(reporter_id)
                    .expect("registered reporter must retain status")
                    .last_outcome,
                LastDiscoveryOutcome::Succeeded { .. }
            ) {
                break;
            }
            std::thread::sleep(Duration::from_millis(1));
        }

        assert_eq!(discoveries.load(Ordering::Relaxed), 1);
        assert!(matches!(
            app.world()
                .resource::<DiscoveryStatus>()
                .reporter_status(reporter_id)
                .expect("registered reporter must retain status")
                .last_outcome,
            LastDiscoveryOutcome::Succeeded { .. }
        ));
        let expected_capacity = IoTaskPool::get().thread_num().saturating_sub(1).clamp(1, 2);
        assert_eq!(
            DiscoveryLimits::default()
                .effective_max_concurrent_jobs()
                .expect("test initializes the I/O task pool")
                .get(),
            expected_capacity
        );
    }

    #[test]
    fn prior_success_remains_visible_across_queued_running_idle_and_disabled_activity() {
        initialize_io_task_pool();
        let mut app = App::new();
        app.add_plugins(RiggingPlugin);
        app.world_mut()
            .resource_mut::<DiscoveryLimits>()
            .set_max_concurrent_jobs(NonZeroUsize::MIN);
        let (started_sender, started_receiver) = channel();
        let (blocker_gate, blocker_release) = background_job_gate(
            "blocker",
            started_sender.clone(),
            DiscoveryProgress::Indeterminate,
        );
        let (subject_gate, subject_release) =
            background_job_gate("subject", started_sender, DiscoveryProgress::Indeterminate);
        let blocker = app.add_device_reporter(
            GatedBackgroundReporter {
                background_job_gate: blocker_gate,
            },
            ReporterRegistration::optional(
                DiscoveryCadence::OnDemand,
                ReporterActivation::Disabled,
                ReporterCoverage::MatchingEvidenceOnly,
            ),
        );
        let subject = app.add_device_reporter(
            OutcomeThenGatedReporter {
                first_discovery_outcome: FirstDiscoveryOutcome::Succeeded,
                discoveries:             Arc::new(AtomicUsize::new(0)),
                runs:                    0,
                background_job_gate:     subject_gate,
            },
            ReporterRegistration::optional(
                DiscoveryCadence::OnDemand,
                ReporterActivation::Enabled,
                ReporterCoverage::MatchingEvidenceOnly,
            ),
        );

        app.update();
        app.update();
        assert_successful_reporter_status(&app, subject, ReporterActivityExpectation::Idle, 1);

        {
            let mut discovery_control = app.world_mut().resource_mut::<DiscoveryControl>();
            discovery_control
                .enable(blocker)
                .expect("registered optional reporter must enable");
            discovery_control
                .request(subject)
                .expect("registered reporter must accept a request");
        }
        let release_blocker = release_jobs_after_start(started_receiver, 1, vec![blocker_release]);
        app.update();
        let (reporter_names, started_receiver) = release_blocker
            .join()
            .expect("gate coordinator must return the blocker reporter name");
        assert_eq!(reporter_names, vec!["blocker"]);
        assert_successful_reporter_status(&app, subject, ReporterActivityExpectation::Queued, 1);

        let release_subject = release_jobs_after_start(started_receiver, 1, vec![subject_release]);
        update_until_completed_batches(&mut app, blocker, 1);
        let (reporter_names, _) = release_subject
            .join()
            .expect("gate coordinator must return the subject reporter name");
        assert_eq!(reporter_names, vec!["subject"]);
        assert_successful_reporter_status(&app, subject, ReporterActivityExpectation::Running, 1);

        update_until_completed_batches(&mut app, subject, 2);
        assert_successful_reporter_status(&app, subject, ReporterActivityExpectation::Idle, 2);

        app.world_mut()
            .resource_mut::<DiscoveryControl>()
            .disable(subject)
            .expect("registered optional reporter must disable");
        app.update();
        assert_successful_reporter_status(&app, subject, ReporterActivityExpectation::Disabled, 2);
    }

    #[test]
    fn running_status_retains_failure_identity_batch_elapsed_and_immediate_progress() {
        initialize_io_task_pool();
        let mut app = App::new();
        app.add_plugins(RiggingPlugin);
        let measured_progress = DiscoveryProgress::Measured {
            completed: 2,
            total:     NonZeroU32::new(4).unwrap_or(NonZeroU32::MIN),
        };
        let (started_sender, started_receiver) = channel();
        let (background_job_gate, background_job_release) =
            background_job_gate("subject", started_sender, measured_progress.clone());
        let reporter = app.add_device_reporter(
            OutcomeThenGatedReporter {
                first_discovery_outcome: FirstDiscoveryOutcome::Failed,
                discoveries: Arc::new(AtomicUsize::new(0)),
                runs: 0,
                background_job_gate,
            },
            ReporterRegistration::optional(
                DiscoveryCadence::OnDemand,
                ReporterActivation::Enabled,
                ReporterCoverage::MatchingEvidenceOnly,
            ),
        );

        app.update();
        app.update();
        assert!(matches!(
            app.world()
                .resource::<DiscoveryStatus>()
                .reporter_status(reporter)
                .expect("registered reporter must retain status"),
            crate::ReporterDiscoveryStatus {
                activity:          ReporterActivity::Idle,
                last_outcome:      LastDiscoveryOutcome::Failed { .. },
                completed_batches: 1,
            }
        ));

        app.world_mut()
            .resource_mut::<DiscoveryControl>()
            .request(reporter)
            .expect("registered reporter must accept a request");
        let release_job =
            release_jobs_after_start(started_receiver, 1, vec![background_job_release]);
        app.update();
        let progress_after = app.world().resource::<DiscoveryLimits>().progress_after();
        assert!(matches!(
            app.world()
                .resource::<DiscoveryStatus>()
                .reporter_status(reporter)
                .expect("reporter lookup identity must select its running status"),
            crate::ReporterDiscoveryStatus {
                activity: ReporterActivity::Running {
                    batch,
                    elapsed,
                    progress: DiscoveryProgress::Indeterminate,
                },
                last_outcome: LastDiscoveryOutcome::Failed { .. },
                completed_batches: 1,
            } if batch.get() == 1 && *elapsed < progress_after
        ));

        let (reporter_names, _) = release_job
            .join()
            .expect("gate coordinator must return the subject reporter name");
        assert_eq!(reporter_names, vec!["subject"]);
        refresh_progress_without_polling_running_jobs(&mut app);
        assert!(matches!(
            app.world()
                .resource::<DiscoveryStatus>()
                .reporter_status(reporter)
                .expect("reporter lookup identity must select its running status"),
            crate::ReporterDiscoveryStatus {
                activity: ReporterActivity::Running {
                    batch,
                    progress,
                    ..
                },
                last_outcome: LastDiscoveryOutcome::Failed { .. },
                completed_batches: 1,
            } if batch.get() == 1 && progress == &measured_progress
        ));

        update_until_completed_batches(&mut app, reporter, 2);
    }

    #[test]
    fn triggers_while_background_reporter_runs_coalesce_into_one_rerun() {
        initialize_io_task_pool();
        let discoveries = Arc::new(AtomicUsize::new(0));
        let now = Instant::now();
        let mut reporter_entry = ReporterEntry::new(
            CountingBackgroundReporter {
                discoveries: Arc::clone(&discoveries),
            },
            ReporterId(0),
            ReporterRegistration::required(
                DiscoveryCadence::OnDemand,
                ReporterCoverage::MatchingEvidenceOnly,
            ),
        );

        assert!(reporter_entry.record_due_signal(
            now,
            DiscoveryRequest::Requested,
            DiscoveryDirtyState::Clean,
        ));
        reporter_entry.queue(DiscoveryBatchId(0), now);
        reporter_entry.prepare(&mut World::new());
        reporter_entry.start_prepared_background_with(|| now);
        assert!(reporter_entry.is_running());
        assert_eq!(discoveries.load(Ordering::Relaxed), 1);

        assert!(!reporter_entry.record_due_signal(
            now,
            DiscoveryRequest::Requested,
            DiscoveryDirtyState::Clean,
        ));
        assert!(!reporter_entry.record_due_signal(
            now,
            DiscoveryRequest::NotRequested,
            DiscoveryDirtyState::Dirty,
        ));
        reporter_entry.next_due = NextDue::At(now);
        assert!(!reporter_entry.record_due_signal(
            now,
            DiscoveryRequest::NotRequested,
            DiscoveryDirtyState::Clean,
        ));
        assert_eq!(discoveries.load(Ordering::Relaxed), 1);

        finish_background_discovery_task(&mut reporter_entry, now);
        assert!(matches!(
            reporter_entry.take_completed(ReporterActivation::Enabled),
            ReporterCompletionAcceptance::Accepted(_)
        ));
        reporter_entry.schedule_after_completion(now);
        assert!(reporter_entry.record_due_signal(
            now,
            DiscoveryRequest::NotRequested,
            DiscoveryDirtyState::Clean,
        ));
        reporter_entry.queue(DiscoveryBatchId(1), now);
        reporter_entry.prepare(&mut World::new());
        reporter_entry.start_prepared_background_with(|| now);
        assert!(reporter_entry.is_running());
        assert_eq!(discoveries.load(Ordering::Relaxed), 2);

        finish_background_discovery_task(&mut reporter_entry, now);
        assert!(matches!(
            reporter_entry.take_completed(ReporterActivation::Enabled),
            ReporterCompletionAcceptance::Accepted(_)
        ));
        reporter_entry.schedule_after_completion(now);
        for _ in 0..3 {
            assert!(!reporter_entry.record_due_signal(
                now,
                DiscoveryRequest::NotRequested,
                DiscoveryDirtyState::Clean,
            ));
        }
        assert_eq!(discoveries.load(Ordering::Relaxed), 2);
    }

    #[test]
    fn new_control_signals_during_background_run_coalesce_into_one_rerun() {
        initialize_io_task_pool();
        let discoveries = Arc::new(AtomicUsize::new(0));
        let mut app = App::new();
        app.add_plugins(RiggingPlugin);
        let (started_sender, started_receiver) = channel();
        let (background_job_gate, background_job_release) =
            background_job_gate("subject", started_sender, DiscoveryProgress::Indeterminate);
        let reporter = app.add_device_reporter(
            OutcomeThenGatedReporter {
                first_discovery_outcome: FirstDiscoveryOutcome::Succeeded,
                discoveries: Arc::clone(&discoveries),
                runs: 0,
                background_job_gate,
            },
            ReporterRegistration::required(
                DiscoveryCadence::OnDemand,
                ReporterCoverage::MatchingEvidenceOnly,
            ),
        );

        app.update();
        app.update();
        app.world_mut()
            .resource_mut::<DiscoveryControl>()
            .request(reporter)
            .expect("registered reporter must accept a request");
        app.update();
        assert_eq!(wait_for_started_jobs(&started_receiver, 1), vec!["subject"]);
        assert_eq!(discoveries.load(Ordering::Relaxed), 2);

        for _ in 0..3 {
            {
                let mut discovery_control = app.world_mut().resource_mut::<DiscoveryControl>();
                discovery_control
                    .request(reporter)
                    .expect("registered reporter must accept a request");
                discovery_control
                    .mark_dirty(reporter)
                    .expect("registered reporter must accept a dirty notification");
            }
            app.update();
        }
        assert_eq!(discoveries.load(Ordering::Relaxed), 2);

        background_job_release.release();
        update_until_completed_batches(&mut app, reporter, 2);
        assert_eq!(wait_for_started_jobs(&started_receiver, 1), vec!["subject"]);
        update_until_completed_batches(&mut app, reporter, 3);
        for _ in 0..3 {
            app.update();
        }

        let reporter_discovery_status = app
            .world()
            .resource::<DiscoveryStatus>()
            .reporter_status(reporter)
            .expect("registered reporter must retain status");
        assert_eq!(discoveries.load(Ordering::Relaxed), 3);
        assert_eq!(reporter_discovery_status.completed_batches, 3);
        assert!(matches!(
            reporter_discovery_status.activity,
            ReporterActivity::Idle
        ));
        assert_no_additional_job_started(&started_receiver);
    }

    #[test]
    fn disabling_running_optional_reporter_suppresses_its_recorded_rerun() {
        initialize_io_task_pool();
        let discoveries = Arc::new(AtomicUsize::new(0));
        let now = Instant::now();
        let mut reporter_entry = ReporterEntry::new(
            CountingBackgroundReporter {
                discoveries: Arc::clone(&discoveries),
            },
            ReporterId(0),
            ReporterRegistration::optional(
                DiscoveryCadence::OnDemand,
                ReporterActivation::Enabled,
                ReporterCoverage::MatchingEvidenceOnly,
            ),
        );

        assert!(reporter_entry.record_due_signal(
            now,
            DiscoveryRequest::Requested,
            DiscoveryDirtyState::Clean,
        ));
        reporter_entry.queue(DiscoveryBatchId(0), now);
        reporter_entry.prepare(&mut World::new());
        reporter_entry.start_prepared_background_with(|| now);
        assert!(reporter_entry.is_running());
        assert_eq!(discoveries.load(Ordering::Relaxed), 1);

        assert!(!reporter_entry.record_due_signal(
            now,
            DiscoveryRequest::Requested,
            DiscoveryDirtyState::Dirty,
        ));
        finish_background_discovery_task(&mut reporter_entry, now);
        assert!(matches!(
            reporter_entry.take_completed(ReporterActivation::Disabled),
            ReporterCompletionAcceptance::Accepted(_)
        ));
        reporter_entry.schedule_after_completion(now);

        assert!(matches!(reporter_entry.state, ReporterRunState::Disabled));
        assert!(!reporter_entry.record_due_signal(
            now,
            DiscoveryRequest::NotRequested,
            DiscoveryDirtyState::Clean,
        ));
        assert_eq!(discoveries.load(Ordering::Relaxed), 1);
    }

    #[test]
    fn registration_returns_distinct_reporter_and_driver_ids() {
        let scans = Arc::new(AtomicUsize::new(0));
        let mut app = App::new();

        let first_reporter = app.add_device_reporter(
            CountingReporter {
                scans: Arc::clone(&scans),
            },
            ReporterRegistration::required(
                DiscoveryCadence::OnDemand,
                ReporterCoverage::MatchingEvidenceOnly,
            ),
        );
        let second_reporter = app.add_device_reporter(
            CountingReporter { scans },
            ReporterRegistration::required(
                DiscoveryCadence::OnDemand,
                ReporterCoverage::MatchingEvidenceOnly,
            ),
        );
        let first_driver = app.add_endpoint_driver(TestDriver);
        let second_driver = app.add_endpoint_driver(TestDriver);

        assert_ne!(first_reporter, second_reporter);
        assert_ne!(first_driver, second_driver);
        let reporters = app.world().resource::<Reporters>();
        assert!(matches!(
            reporters.latest_device_set(first_reporter),
            ReporterDeviceSetState::AwaitingCompleteSet
        ));
        assert!(matches!(
            reporters.latest_device_set(ReporterId(u32::MAX)),
            ReporterDeviceSetState::NotRegistered
        ));
    }

    #[test]
    fn completed_scans_use_returned_reporter_id_and_advance_revision() {
        let scans = Arc::new(AtomicUsize::new(0));
        let mut app = App::new();
        app.add_plugins(RiggingPlugin);
        let reporter_id = app.add_device_reporter(
            CountingReporter { scans },
            ReporterRegistration::required(
                DiscoveryCadence::Periodic {
                    interval: Duration::ZERO,
                },
                ReporterCoverage::MatchingEvidenceOnly,
            ),
        );

        app.update();
        app.update();

        let device_set = available_device_set(
            app.world()
                .resource::<Reporters>()
                .latest_device_set(reporter_id),
        )
        .expect("accepted complete set must remain available");
        assert_eq!(device_set.reporter, reporter_id);
        assert_eq!(device_set.revision.get(), 1);
    }

    #[test]
    fn driver_registry_routes_each_erased_dispatch() -> Result<(), Box<dyn Error>> {
        let mut app = App::new();
        let driver_id = app.add_endpoint_driver(TestDriver);
        let endpoint = display_endpoint()?;
        let role = RoleKey::new("test-driver")?;
        let mut bindings = Bindings::default();
        let hardware_inventory = HardwareInventory::default();
        bindings.register(Binding {
            role: role.clone(),
            endpoint,
            driver: driver_id,
            recovery: RecoveryPolicy::default(),
            retry: RetryOn::NewRevision,
            on_abort: OnAbort::default(),
            on_loss: OnSessionLoss::default(),
            state: RoleState::default(),
            requested: RequestedConfiguration::new(TestConfiguration),
            last_known_good: LastKnownGoodConfiguration::default(),
        })?;

        let start_apply_request = match bindings.role_view(&role)? {
            RoleView::Waiting(waiting_role) => waiting_role.start_requested_apply(
                AttemptId::default(),
                ApplyPermit::in_service(),
                &hardware_inventory,
            )?,
            _ => return Err("registered binding must begin waiting".into()),
        };
        let start = app
            .world_mut()
            .resource_scope::<Drivers, _>(|world, mut drivers| {
                drivers.start_apply(world, start_apply_request)
            });
        let poll_request = match bindings.role_view(&role)? {
            RoleView::Applying(applying_role) => applying_role.poll_request(&hardware_inventory)?,
            _ => return Err("started apply must select applying role view".into()),
        };
        let poll = app
            .world_mut()
            .resource_scope::<Drivers, _>(|world, mut drivers| drivers.poll(world, poll_request));
        match bindings.role_view(&role)? {
            RoleView::Applying(mut applying_role) => {
                applying_role.finish(AttemptOutcome::Succeeded);
            },
            _ => return Err("poll requires applying role view".into()),
        }
        let capture_request = match bindings.role_view(&role)? {
            RoleView::Ready(ready_role) => ready_role.capture_request(&hardware_inventory)?,
            _ => return Err("successful apply must make role ready".into()),
        };
        let capture = app
            .world_mut()
            .resource_scope::<Drivers, _>(|world, mut drivers| {
                drivers.capture(world, capture_request)
            });

        assert!(matches!(
            capture,
            Ok(CaptureOutcome::ReadFailed(DeviceAccessError::Absent { .. }))
        ));
        assert_eq!(start, Ok(()));
        assert!(matches!(poll, Ok(AttemptProgress::Pending)));

        Ok(())
    }

    #[test]
    fn reflection_cannot_construct_apply_permit() {
        let mut dynamic_permit = DynamicTupleStruct::default();
        dynamic_permit.insert(Purpose::InService);

        assert!(ApplyPermit::from_reflect(&dynamic_permit).is_none());
    }

    #[test]
    fn reflection_cannot_construct_driver_id() {
        let mut dynamic_driver_id = DynamicTupleStruct::default();
        dynamic_driver_id.insert(0_u32);

        assert!(DriverId::from_reflect(&dynamic_driver_id).is_none());
    }

    #[test]
    fn apply_permit_reports_its_kernel_minted_purpose() {
        assert!(ApplyPermit::in_service().allows_in_service_use());
        assert!(!ApplyPermit::restore_only().allows_in_service_use());
    }

    #[test]
    fn scheme_registration_works_before_and_after_rigging_plugin() -> Result<(), Box<dyn Error>> {
        let scheme = SchemeName::new("edid-serial")?;
        let mut before = App::new();
        before.add_plugins((FirstSchemePlugin(scheme.clone()), RiggingPlugin));
        assert!(
            before
                .world()
                .resource::<RegisteredSchemes>()
                .contains(&scheme)
        );

        let mut after = App::new();
        after.add_plugins((RiggingPlugin, FirstSchemePlugin(scheme.clone())));
        assert!(
            after
                .world()
                .resource::<RegisteredSchemes>()
                .contains(&scheme)
        );

        Ok(())
    }

    #[test]
    fn duplicate_scheme_plugins_keep_one_registered_name() -> Result<(), Box<dyn Error>> {
        let scheme = SchemeName::new("edid-serial")?;
        let device_key = DeviceKey {
            kind: DeviceKind::Display,
            id:   DeviceIdSource::Reported {
                scheme: scheme.clone(),
                value:  ReportedId::new("DELL-U2723QE-9J4K2H3")?,
            },
        };
        let mut first_then_second = App::new();
        first_then_second.add_plugins((
            RiggingPlugin,
            FirstSchemePlugin(scheme.clone()),
            SecondSchemePlugin(scheme.clone()),
        ));

        let first_registered_schemes = first_then_second.world().resource::<RegisteredSchemes>();
        assert!(first_registered_schemes.contains(&scheme));
        assert_eq!(first_registered_schemes.count(), 1);
        first_registered_schemes.validate(&device_key)?;

        let mut second_then_first = App::new();
        second_then_first.add_plugins((
            RiggingPlugin,
            SecondSchemePlugin(scheme.clone()),
            FirstSchemePlugin(scheme.clone()),
        ));

        let second_registered_schemes = second_then_first.world().resource::<RegisteredSchemes>();
        assert!(second_registered_schemes.contains(&scheme));
        assert_eq!(second_registered_schemes.count(), 1);
        second_registered_schemes.validate(&device_key)?;

        Ok(())
    }

    fn display_endpoint() -> Result<DeviceEndpoint, Box<dyn Error>> {
        Ok(DeviceEndpoint {
            device: DeviceKey {
                kind: DeviceKind::Display,
                id:   DeviceIdSource::Reported {
                    scheme: SchemeName::new("edid-serial")?,
                    value:  ReportedId::new("DELL-U2723QE-9J4K2H3")?,
                },
            },
            id:     EndpointId::Whole,
        })
    }
}
