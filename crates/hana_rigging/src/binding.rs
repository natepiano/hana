use std::collections::HashMap;
use std::collections::HashSet;
use std::collections::VecDeque;
use std::num::NonZeroUsize;

use bevy::ecs::reflect::ReflectResource;
use bevy::prelude::Reflect;
use bevy::prelude::Resource;
use thiserror::Error;

use crate::ApplyPermit;
use crate::AttemptId;
use crate::AttemptOutcome;
use crate::CaptureOutcome;
use crate::DeviceEndpoint;
use crate::DeviceKey;
use crate::DriverId;
use crate::LastKnownGoodConfiguration;
use crate::OnAbort;
use crate::OnSessionLoss;
use crate::RecoveryPolicy;
use crate::RetryOn;
use crate::RoleKey;
use crate::RoleState;

const DEFAULT_PENDING_TRANSITION_CAPACITY: usize = 4_096;

/// One authored role binding, including its durable endpoint and driver-specific configuration.
///
/// A `Binding` keeps the application role separate from the device entity that may currently
/// represent `endpoint.device`. This lets a window, camera slot, or panel key retain its authored
/// configuration while the physical unit is absent, without treating a process-local `DeviceId`
/// as durable identity.
#[derive(Reflect)]
pub struct Binding {
    /// Application role that remains stable while devices leave and return.
    pub role:            RoleKey,
    /// Durable device key and provider-defined part that this role exclusively owns in v1.
    pub endpoint:        DeviceEndpoint,
    /// Registered endpoint driver that receives this role's erased configuration.
    pub driver:          DriverId,
    /// Retention rule applied when the device supplying this endpoint departs.
    pub recovery:        RecoveryPolicy,
    /// Retry rule applied after an endpoint driver reports a recoverable failure.
    pub retry:           RetryOn,
    /// Response selected when an in-flight operation is abandoned by a new device report.
    pub on_abort:        OnAbort,
    /// Response selected when a still-present endpoint loses its local session.
    pub on_loss:         OnSessionLoss,
    /// Current role lifecycle state; `Bindings` is its sole live writer after registration.
    pub state:           RoleState,
    /// Authored driver target that describes what the application wants to reach.
    pub requested:       RequestedConfiguration,
    /// Driver value a safe readback most recently proved was on this endpoint.
    pub last_known_good: LastKnownGoodConfiguration,
}

/// Authored driver configuration held without exposing the concrete configuration type to the
/// kernel.
///
/// The concrete configuration remains owned by the endpoint driver. `RequestedConfiguration`
/// exists because a display placement and a camera format can share a `DeviceKey` while requiring
/// unrelated driver types and routing rules.
#[derive(Reflect)]
pub struct RequestedConfiguration(
    #[reflect(ignore, default = "default_erased_configuration")] Box<dyn Reflect>,
);

impl RequestedConfiguration {
    /// Erase one driver-specific value while retaining it as authored role intent.
    #[must_use]
    pub fn new(configuration: impl Reflect) -> Self { Self(Box::new(configuration)) }

    pub(crate) fn as_reflect(&self) -> &dyn Reflect { self.0.as_ref() }
}

fn default_erased_configuration() -> Box<dyn Reflect> { Box::new(()) }

/// Configuration currently available to an offline UI or authoring workflow.
///
/// A proven value takes precedence because it describes the endpoint state a safe readback
/// observed. When no readback has succeeded, the authored request remains useful for presenting
/// the role's intended value without fabricating endpoint evidence.
pub enum AvailableConfiguration<'a> {
    /// A safe readback established this value on the endpoint.
    LastKnownGood(&'a dyn Reflect),
    /// No readback established a value, so this is the authored target instead.
    Requested(&'a dyn Reflect),
}

/// Role records, endpoint ownership, and bounded lifecycle handoff owned by the kernel.
///
/// `Bindings` retains authored intent even when no live device entity exists. Its private reverse
/// indexes make duplicate endpoint ownership unavailable through checked registration methods,
/// while `PendingBindingTransitions` retains only the next frame's lifecycle work.
#[derive(Default, Resource, Reflect)]
#[reflect(Resource)]
pub struct Bindings {
    #[reflect(ignore, default = "default_bindings_by_role")]
    by_role:                  HashMap<RoleKey, Binding>,
    #[reflect(ignore, default = "default_owner_by_endpoint")]
    owner_by_endpoint:        HashMap<DeviceEndpoint, RoleKey>,
    #[reflect(ignore, default = "default_roles_by_device")]
    roles_by_device:          HashMap<DeviceKey, Vec<RoleKey>>,
    #[reflect(ignore, default = "default_unreadable_configuration")]
    unreadable_configuration: HashSet<RoleKey>,
    #[reflect(ignore, default = "PendingBindingTransitions::default")]
    pending_transitions:      PendingBindingTransitions,
    #[reflect(ignore, default = "default_transition_sequence")]
    next_transition_sequence: u64,
}

fn default_bindings_by_role() -> HashMap<RoleKey, Binding> { HashMap::new() }

fn default_owner_by_endpoint() -> HashMap<DeviceEndpoint, RoleKey> { HashMap::new() }

fn default_roles_by_device() -> HashMap<DeviceKey, Vec<RoleKey>> { HashMap::new() }

fn default_unreadable_configuration() -> HashSet<RoleKey> { HashSet::new() }

const fn default_transition_sequence() -> u64 { 0 }

/// Monotonic order attached to lifecycle handoff entries.
///
/// This sequence orders changes submitted before a frame drain; it is not a historical log and
/// consumers cannot use it to recover work after the bounded handoff releases an entry.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Reflect)]
#[reflect(opaque)]
pub struct BindingTransitionSequence(u64);

impl BindingTransitionSequence {
    /// Return the ordering number assigned when one effective binding operation was accepted.
    #[must_use]
    pub const fn get(self) -> u64 { self.0 }
}

/// One accepted binding lifecycle change awaiting the next frame's internal processing.
#[derive(Debug, PartialEq, Eq, Reflect)]
pub enum BindingTransition {
    /// A newly registered role needs a binding entity during the next lifecycle stage.
    Registered {
        /// Ordering number for this accepted operation.
        sequence: BindingTransitionSequence,
        /// Authored role whose binding was registered.
        role:     RoleKey,
    },
    /// A replacement displaced a prior binding whose in-flight work is handled later.
    Replaced {
        /// Ordering number for this accepted operation.
        sequence: BindingTransitionSequence,
        /// Authored role whose binding was replaced.
        role:     RoleKey,
    },
    /// A retired role needs entity cleanup and any later attempt-abort processing.
    Retired {
        /// Ordering number for this accepted operation.
        sequence: BindingTransitionSequence,
        /// Authored role whose binding was retired.
        role:     RoleKey,
    },
}

struct PendingBindingTransitions {
    capacity: NonZeroUsize,
    queue:    VecDeque<BindingTransition>,
}

impl Default for PendingBindingTransitions {
    fn default() -> Self {
        Self {
            capacity: NonZeroUsize::new(DEFAULT_PENDING_TRANSITION_CAPACITY)
                .unwrap_or(NonZeroUsize::MIN),
            queue:    VecDeque::new(),
        }
    }
}

impl PendingBindingTransitions {
    fn has_capacity(&self) -> bool { self.queue.len() < self.capacity.get() }

    fn push(&mut self, binding_transition: BindingTransition) {
        self.queue.push_back(binding_transition);
    }
}

impl Bindings {
    /// Register one authored role after confirming that no prior role owns its endpoint.
    ///
    /// # Errors
    ///
    /// Returns `BindingError` when the role already exists, another role owns the endpoint, or
    /// the bounded lifecycle handoff cannot retain this registration without dropping work.
    pub fn register(&mut self, mut binding: Binding) -> Result<(), BindingError> {
        if self.by_role.contains_key(&binding.role) {
            return Err(BindingError::RoleAlreadyBound { role: binding.role });
        }
        if let Some(owner) = self.owner_by_endpoint.get(&binding.endpoint) {
            return Err(BindingError::EndpointAlreadyOwned {
                endpoint: binding.endpoint,
                owner:    owner.clone(),
            });
        }
        let reserved_transition = self.reserve_transition()?;

        binding.state = RoleState::Waiting;
        let role = binding.role.clone();
        let endpoint = binding.endpoint.clone();
        let device_key = endpoint.device.clone();
        self.owner_by_endpoint.insert(endpoint, role.clone());
        self.roles_by_device
            .entry(device_key)
            .or_default()
            .push(role.clone());
        self.by_role.insert(role.clone(), binding);
        self.enqueue(BindingTransitionKind::Registered, role, reserved_transition);

        Ok(())
    }

    /// Replace one binding only after proving its new endpoint is not owned by another role.
    ///
    /// The old endpoint remains owned until the proposed endpoint and transition handoff both
    /// pass their checks, so an error cannot leave a role unbound or corrupt either reverse index.
    ///
    /// # Errors
    ///
    /// Returns `BindingError` when no old binding exists, a different role owns the proposed
    /// endpoint, or the transition handoff has no space for this replacement.
    pub fn replace(&mut self, mut binding: Binding) -> Result<Binding, BindingError> {
        let old_binding =
            self.by_role
                .get(&binding.role)
                .ok_or_else(|| BindingError::RoleNotBound {
                    role: binding.role.clone(),
                })?;
        if let Some(owner) = self.owner_by_endpoint.get(&binding.endpoint)
            && owner != &binding.role
        {
            return Err(BindingError::EndpointAlreadyOwned {
                endpoint: binding.endpoint,
                owner:    owner.clone(),
            });
        }
        let reserved_transition = self.reserve_transition()?;

        binding.state = RoleState::Waiting;
        let role = binding.role.clone();
        let old_endpoint = old_binding.endpoint.clone();
        let new_endpoint = binding.endpoint.clone();
        let displaced = self
            .by_role
            .insert(role.clone(), binding)
            .ok_or_else(|| BindingError::RoleNotBound { role: role.clone() })?;

        if old_endpoint != new_endpoint {
            let new_device_key = new_endpoint.device.clone();
            self.owner_by_endpoint.remove(&old_endpoint);
            self.remove_role_from_device(&old_endpoint.device, &role);
            self.owner_by_endpoint.insert(new_endpoint, role.clone());
            self.roles_by_device
                .entry(new_device_key)
                .or_default()
                .push(role.clone());
        }
        self.unreadable_configuration.remove(&role);
        self.enqueue(BindingTransitionKind::Replaced, role, reserved_transition);

        Ok(displaced)
    }

    /// Retire an authored role and remove every ownership index entry that selected it.
    ///
    /// # Errors
    ///
    /// Returns `BindingError::PendingTransitionCapacityReached` when this effective retirement
    /// cannot be retained for later lifecycle processing. Retiring a role that is already absent
    /// succeeds with `RetirementOutcome::AlreadyUnbound` and creates no transition.
    pub fn retire(&mut self, role: &RoleKey) -> Result<RetirementOutcome, BindingError> {
        if !self.by_role.contains_key(role) {
            return Ok(RetirementOutcome::AlreadyUnbound);
        }
        let reserved_transition = self.reserve_transition()?;

        let mut binding = self
            .by_role
            .remove(role)
            .ok_or_else(|| BindingError::RoleNotBound { role: role.clone() })?;
        self.owner_by_endpoint.remove(&binding.endpoint);
        self.remove_role_from_device(&binding.endpoint.device, role);
        self.unreadable_configuration.remove(role);
        binding.state = RoleState::Retired;
        self.enqueue(
            BindingTransitionKind::Retired,
            role.clone(),
            reserved_transition,
        );

        Ok(RetirementOutcome::Retired(binding))
    }

    /// Borrow one binding without exposing a mutable path around its role lifecycle views.
    ///
    /// # Errors
    ///
    /// Returns `BindingError::RoleNotBound` when the role has no retained authored binding.
    pub fn binding(&self, role: &RoleKey) -> Result<&Binding, BindingError> {
        self.by_role
            .get(role)
            .ok_or_else(|| BindingError::RoleNotBound { role: role.clone() })
    }

    /// Iterate every retained role whose endpoint names `device_key`.
    ///
    /// Several roles can address different endpoints of one device, so callers receive every
    /// role instead of a convenient but unsafe first match.
    pub fn roles_for(&self, device_key: &DeviceKey) -> impl Iterator<Item = &RoleKey> {
        self.roles_by_device.get(device_key).into_iter().flatten()
    }

    /// Select the one lifecycle view whose methods are valid for this stored role state.
    ///
    /// # Errors
    ///
    /// Returns `BindingError::RoleNotBound` when the requested application role has no binding.
    #[cfg_attr(
        not(test),
        expect(
            dead_code,
            reason = "phase 11 attempt systems select state-issued binding role views"
        )
    )]
    pub(crate) fn role_view(&mut self, role: &RoleKey) -> Result<RoleView<'_>, BindingError> {
        let unreadable_configuration = &mut self.unreadable_configuration;
        let binding = self
            .by_role
            .get_mut(role)
            .ok_or_else(|| BindingError::RoleNotBound { role: role.clone() })?;

        Ok(match binding.state {
            RoleState::Waiting => RoleView::Waiting(WaitingRole { binding }),
            RoleState::Ready => RoleView::Ready(ReadyRole {
                binding,
                unreadable_configuration,
            }),
            RoleState::Applying(_) => RoleView::Applying(ApplyingRole { binding }),
            RoleState::Retired => RoleView::Retired,
        })
    }

    /// Return the value an offline authoring interface can show for one retained role.
    ///
    /// # Errors
    ///
    /// Returns `BindingError::RoleNotBound` when the role was never registered or was retired.
    pub fn configuration_for(
        &self,
        role: &RoleKey,
    ) -> Result<AvailableConfiguration<'_>, BindingError> {
        let binding = self.binding(role)?;
        Ok(match &binding.last_known_good {
            LastKnownGoodConfiguration::Known(configuration) => {
                AvailableConfiguration::LastKnownGood(configuration.as_ref())
            },
            LastKnownGoodConfiguration::NotEstablished => {
                AvailableConfiguration::Requested(binding.requested.as_reflect())
            },
        })
    }

    /// Change the bounded lifecycle handoff capacity without discarding already accepted work.
    ///
    /// # Errors
    ///
    /// Returns `BindingCapacityError` when the requested capacity is smaller than the number of
    /// lifecycle transitions currently awaiting the next frame drain.
    pub fn set_pending_transition_capacity(
        &mut self,
        capacity: NonZeroUsize,
    ) -> Result<(), BindingCapacityError> {
        let pending = self.pending_transitions.queue.len();
        if capacity.get() < pending {
            return Err(BindingCapacityError::BelowPendingCount { capacity, pending });
        }

        self.pending_transitions.capacity = capacity;
        Ok(())
    }

    #[cfg_attr(
        not(test),
        expect(
            dead_code,
            reason = "phase 9 moves binding transitions into its frame batch"
        )
    )]
    pub(crate) fn take_pending_transitions(&mut self) -> VecDeque<BindingTransition> {
        std::mem::take(&mut self.pending_transitions.queue)
    }

    fn reserve_transition(&self) -> Result<ReservedBindingTransition, BindingError> {
        if !self.pending_transitions.has_capacity() {
            return Err(BindingError::PendingTransitionCapacityReached);
        }
        let next_transition_sequence = self
            .next_transition_sequence
            .checked_add(1)
            .ok_or(BindingError::TransitionSequenceExhausted)?;

        Ok(ReservedBindingTransition {
            sequence: BindingTransitionSequence(self.next_transition_sequence),
            next_transition_sequence,
        })
    }

    fn enqueue(
        &mut self,
        binding_transition_kind: BindingTransitionKind,
        role: RoleKey,
        reserved_transition: ReservedBindingTransition,
    ) {
        let ReservedBindingTransition {
            sequence,
            next_transition_sequence,
        } = reserved_transition;
        self.next_transition_sequence = next_transition_sequence;
        let binding_transition = match binding_transition_kind {
            BindingTransitionKind::Registered => BindingTransition::Registered { sequence, role },
            BindingTransitionKind::Replaced => BindingTransition::Replaced { sequence, role },
            BindingTransitionKind::Retired => BindingTransition::Retired { sequence, role },
        };
        self.pending_transitions.push(binding_transition);
    }

    fn remove_role_from_device(&mut self, device_key: &DeviceKey, role: &RoleKey) {
        let remove_device_entry = if let Some(roles) = self.roles_by_device.get_mut(device_key) {
            roles.retain(|stored_role| stored_role != role);
            roles.is_empty()
        } else {
            false
        };
        if remove_device_entry {
            self.roles_by_device.remove(device_key);
        }
    }
}

enum BindingTransitionKind {
    Registered,
    Replaced,
    Retired,
}

struct ReservedBindingTransition {
    sequence:                 BindingTransitionSequence,
    next_transition_sequence: u64,
}

/// Result of retiring one role while distinguishing a repeated request from an effective change.
pub enum RetirementOutcome {
    /// The retained binding was removed and marked retired before it was returned.
    Retired(Binding),
    /// No binding remained for this role, so no lifecycle work was added.
    AlreadyUnbound,
}

/// Recoverable failure from a checked binding operation or state-issued request.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum BindingError {
    /// The submitted role already has an authored binding whose endpoint ownership must remain.
    #[error("role `{role}` is already bound")]
    RoleAlreadyBound {
        /// Existing application role that rejected a second binding record.
        role: RoleKey,
    },
    /// An operation required a binding for this role, but none remains registered.
    #[error("role `{role}` is not bound")]
    RoleNotBound {
        /// Application role that did not select a retained binding record.
        role: RoleKey,
    },
    /// A different role already owns the proposed endpoint, so two drivers cannot race it.
    #[error("endpoint `{endpoint:?}` is already owned by role `{owner}`")]
    EndpointAlreadyOwned {
        /// Endpoint the operation proposed for a second role.
        endpoint: DeviceEndpoint,
        /// Retained role whose binding already owns `endpoint`.
        owner:    RoleKey,
    },
    /// The lifecycle handoff is full, so accepting another authored mutation would lose work.
    #[error("pending binding transition capacity has been reached")]
    PendingTransitionCapacityReached,
    /// The next lifecycle handoff sequence cannot advance without reusing a prior transition id.
    #[error("binding transition sequence is exhausted")]
    TransitionSequenceExhausted,
    /// An apply of authored intent requires the permit that authorizes in-service use.
    #[error("requested configuration requires an in-service apply permit")]
    RequestedConfigurationRequiresInServicePermit,
    /// A restore from observed endpoint state requires the restore-only authorization purpose.
    #[error("last-known-good configuration requires a restore-only apply permit")]
    LastKnownGoodConfigurationRequiresRestoreOnlyPermit,
    /// A restore was requested before a safe readback established a value to restore.
    #[error("role `{role}` has no last-known-good configuration")]
    LastKnownGoodNotEstablished {
        /// Role whose configuration remains authored intent rather than endpoint evidence.
        role: RoleKey,
    },
    /// The configured device is offline, so passive discovery may observe it but no driver call
    /// may be issued for its endpoint.
    #[error("configured device `{device_key:?}` is offline")]
    ConfiguredDeviceOffline {
        /// Durable key whose authored offline mode blocks operational requests.
        device_key: DeviceKey,
    },
    /// A driver previously established that this endpoint cannot provide a safe configuration.
    #[error("role `{role}` has no readable endpoint configuration")]
    ConfigurationNotReadable {
        /// Role whose driver returned `CaptureOutcome::NotReadable`.
        role: RoleKey,
    },
}

/// Failure from attempting to lower lifecycle handoff capacity below already pending work.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum BindingCapacityError {
    /// The requested capacity cannot retain the transitions already waiting for a frame drain.
    #[error("pending transition count {pending} exceeds requested capacity {capacity}")]
    BelowPendingCount {
        /// Capacity the caller requested for future lifecycle changes.
        capacity: NonZeroUsize,
        /// Number of transitions that must remain retained before the next drain.
        pending:  usize,
    },
}

/// State-selected access to one binding; only the contained view exposes valid operations.
pub enum RoleView<'a> {
    /// The role has no usable endpoint and may start a newly authorized operation.
    Waiting(WaitingRole<'a>),
    /// The role has reached its target and may ask for a safe configuration readback.
    Ready(ReadyRole<'a>),
    /// The role has an in-flight driver operation and may poll, abort, or finish it.
    Applying(ApplyingRole<'a>),
    /// The role was retired, so no driver operation can be issued.
    Retired,
}

/// View of a role that has no usable endpoint operation in progress.
#[cfg_attr(
    not(test),
    expect(
        dead_code,
        reason = "phase 11 attempt systems mint waiting-role apply requests"
    )
)]
pub struct WaitingRole<'a> {
    binding: &'a mut Binding,
}

#[cfg_attr(
    not(test),
    expect(
        dead_code,
        reason = "phase 11 attempt systems start authorized waiting-role applies"
    )
)]
impl<'a> WaitingRole<'a> {
    /// Start an authorized apply from the binding's authored requested configuration.
    ///
    /// # Errors
    ///
    /// Returns `BindingError::ConfiguredDeviceOffline` when inventory marks this durable device
    /// offline, which prevents the request before a driver can observe it.
    pub(crate) fn start_requested_apply(
        self,
        attempt: AttemptId,
        permit: ApplyPermit,
        hardware_inventory: &HardwareInventory,
    ) -> Result<StartApplyRequest<'a>, BindingError> {
        if !permit.allows_in_service_use() {
            return Err(BindingError::RequestedConfigurationRequiresInServicePermit);
        }
        hardware_inventory.ensure_operational(&self.binding.endpoint.device)?;
        Ok(StartApplyRequest {
            binding: self.binding,
            configuration_source: ApplyConfigurationSource::Requested,
            attempt,
            permit,
        })
    }

    /// Start an authorized restore from the value a safe readback previously established.
    ///
    /// # Errors
    ///
    /// Returns `BindingError::LastKnownGoodNotEstablished` until a successful safe readback has
    /// supplied an endpoint value, or `BindingError::ConfiguredDeviceOffline` for offline
    /// inventory that may be discovered passively but may not receive output.
    pub(crate) fn start_last_known_good_restore(
        self,
        attempt: AttemptId,
        permit: ApplyPermit,
        hardware_inventory: &HardwareInventory,
    ) -> Result<StartApplyRequest<'a>, BindingError> {
        if permit.allows_in_service_use() {
            return Err(BindingError::LastKnownGoodConfigurationRequiresRestoreOnlyPermit);
        }
        hardware_inventory.ensure_operational(&self.binding.endpoint.device)?;
        self.binding.last_known_good.as_reflect().map_err(|_| {
            BindingError::LastKnownGoodNotEstablished {
                role: self.binding.role.clone(),
            }
        })?;
        Ok(StartApplyRequest {
            binding: self.binding,
            configuration_source: ApplyConfigurationSource::LastKnownGood,
            attempt,
            permit,
        })
    }
}

/// View of a role that has a usable endpoint and no in-flight driver operation.
#[cfg_attr(
    not(test),
    expect(
        dead_code,
        reason = "phase 10 reconciliation captures ready-role configuration"
    )
)]
pub struct ReadyRole<'a> {
    binding:                  &'a mut Binding,
    unreadable_configuration: &'a mut HashSet<RoleKey>,
}

#[cfg_attr(
    not(test),
    expect(
        dead_code,
        reason = "phase 10 reconciliation records ready-role capture outcomes"
    )
)]
impl<'a> ReadyRole<'a> {
    /// Mint the only capture request accepted by driver dispatch.
    ///
    /// # Errors
    ///
    /// Returns `BindingError::ConfigurationNotReadable` after the driver permanently declined a
    /// safe readback, or `BindingError::ConfiguredDeviceOffline` before any driver call for an
    /// offline configured endpoint.
    pub(crate) fn capture_request(
        self,
        hardware_inventory: &HardwareInventory,
    ) -> Result<CaptureRequest<'a>, BindingError> {
        hardware_inventory.ensure_operational(&self.binding.endpoint.device)?;
        if self.unreadable_configuration.contains(&self.binding.role) {
            return Err(BindingError::ConfigurationNotReadable {
                role: self.binding.role.clone(),
            });
        }

        Ok(CaptureRequest {
            role:     &self.binding.role,
            driver:   self.binding.driver,
            endpoint: &self.binding.endpoint,
        })
    }

    /// Record what one safe driver readback established without treating an apply target as proof.
    pub(crate) fn record_capture(
        &mut self,
        capture_outcome: CaptureOutcome<LastKnownGoodConfiguration>,
    ) {
        match capture_outcome {
            CaptureOutcome::Read(last_known_good) => {
                self.binding.last_known_good = last_known_good;
            },
            CaptureOutcome::NotReadable => {
                self.unreadable_configuration
                    .insert(self.binding.role.clone());
            },
            CaptureOutcome::ReadFailed(_) => {},
        }
    }
}

/// View of a role whose driver operation is in flight.
#[cfg_attr(
    not(test),
    expect(
        dead_code,
        reason = "phase 11 attempt systems poll abort and finish applying roles"
    )
)]
pub struct ApplyingRole<'a> {
    binding: &'a mut Binding,
}

#[cfg_attr(
    not(test),
    expect(
        dead_code,
        reason = "phase 11 attempt systems poll abort and finish applying roles"
    )
)]
impl<'a> ApplyingRole<'a> {
    /// Mint the only poll request accepted by driver dispatch for this in-flight attempt.
    ///
    /// # Errors
    ///
    /// Returns `BindingError::ConfiguredDeviceOffline` when inventory switched this device to
    /// offline before the next poll, preventing the driver from continuing the operation.
    pub(crate) fn poll_request(
        self,
        hardware_inventory: &HardwareInventory,
    ) -> Result<PollRequest<'a>, BindingError> {
        hardware_inventory.ensure_operational(&self.binding.endpoint.device)?;
        let RoleState::Applying(attempt) = self.binding.state else {
            return Err(BindingError::RoleNotBound {
                role: self.binding.role.clone(),
            });
        };
        Ok(PollRequest {
            role: &self.binding.role,
            driver: self.binding.driver,
            endpoint: &self.binding.endpoint,
            attempt,
        })
    }

    /// Stop the in-flight operation and return the role to the waiting lifecycle state.
    pub(crate) const fn abort(&mut self) { self.binding.state = RoleState::Waiting; }

    /// Finish the in-flight operation, making only a successful apply ready for safe readback.
    pub(crate) fn finish(&mut self, attempt_outcome: AttemptOutcome) {
        self.binding.state = match attempt_outcome {
            AttemptOutcome::Succeeded | AttemptOutcome::Substituted => RoleState::Ready,
            AttemptOutcome::Failed(_) | AttemptOutcome::Aborted => RoleState::Waiting,
        };
    }
}

/// State-issued permission to ask one driver for a safe endpoint configuration readback.
///
/// Its fields stay private so application code cannot choose a `DriverId` and endpoint without a
/// `ReadyRole` proving that the binding reached the state where capture is meaningful.
pub struct CaptureRequest<'a> {
    #[expect(
        dead_code,
        reason = "phase 10 reconciliation records the capture request role in diagnostics"
    )]
    pub(crate) role:     &'a RoleKey,
    pub(crate) driver:   DriverId,
    pub(crate) endpoint: &'a DeviceEndpoint,
}

/// State-issued permission to start one asynchronous driver apply.
///
/// The request borrows the selected configuration source, so a later mutation cannot replace the
/// driver's target between lifecycle authorization and erased driver dispatch.
pub struct StartApplyRequest<'a> {
    pub(crate) binding:              &'a mut Binding,
    pub(crate) configuration_source: ApplyConfigurationSource,
    pub(crate) attempt:              AttemptId,
    pub(crate) permit:               ApplyPermit,
}

/// Configuration source paired with the authorization purpose that permits its dispatch.
#[cfg_attr(
    not(test),
    expect(
        dead_code,
        reason = "phase 11 attempt systems dispatch state-issued apply configuration sources"
    )
)]
pub(crate) enum ApplyConfigurationSource {
    Requested,
    LastKnownGood,
}

#[cfg_attr(
    not(test),
    expect(
        dead_code,
        reason = "phase 11 attempt systems borrow state-issued apply configurations"
    )
)]
impl ApplyConfigurationSource {
    pub(crate) fn configuration(self, binding: &Binding) -> Result<&dyn Reflect, BindingError> {
        match self {
            Self::Requested => Ok(binding.requested.as_reflect()),
            Self::LastKnownGood => binding.last_known_good.as_reflect().map_err(|_| {
                BindingError::LastKnownGoodNotEstablished {
                    role: binding.role.clone(),
                }
            }),
        }
    }
}

/// State-issued permission to poll one in-flight driver apply.
///
/// The request retains the role and endpoint even though the driver trait polls by attempt id,
/// so reconciliation can compare the token with current resolution before dispatch.
pub struct PollRequest<'a> {
    #[expect(
        dead_code,
        reason = "phase 11 attempt processing compares the poll request role with resolution"
    )]
    pub(crate) role:     &'a RoleKey,
    pub(crate) driver:   DriverId,
    #[expect(
        dead_code,
        reason = "phase 11 attempt processing compares the poll request endpoint with resolution"
    )]
    pub(crate) endpoint: &'a DeviceEndpoint,
    pub(crate) attempt:  AttemptId,
}

/// Whether authored inventory permits driver operations for a configured device.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Reflect)]
pub enum ConfiguredDeviceMode {
    /// The kernel may later ask a driver to capture, apply, and poll this device's endpoints.
    Managed,
    /// Reporters may enumerate the device passively, but no driver operation may touch it.
    Offline,
}

/// Authored device inventory entry that exists independently of reporter activation and entities.
#[derive(Clone, Debug, PartialEq, Eq, Reflect)]
pub struct ConfiguredDevice {
    /// Durable identity the application authored without creating a live device entity.
    pub key:  DeviceKey,
    /// Operational rule that leaves passive connection evidence visible when offline.
    pub mode: ConfiguredDeviceMode,
}

/// Connectivity conclusion retained for one authored device without changing its operational mode.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Reflect)]
pub enum ConfiguredDeviceConnection {
    /// No enabled reporter completed a discovery capable of observing this authored key.
    NotObserved,
    /// Current passive reporter evidence contains this authored key.
    Present,
    /// Relevant complete reporter evidence omitted this key.
    Absent,
    /// Relevant evidence expired or discovery failed before absence could be established.
    Unreachable,
}

/// Authored durable hardware inventory and passive connectivity conclusions.
///
/// Adding an entry does not register, enable, or run a reporter, and it never creates a live
/// `DeviceId`. Reconciliation updates `ConfiguredDeviceConnection` from retained reporter
/// evidence while this resource keeps offline operation separate from connection visibility.
#[derive(Default, Resource, Reflect)]
#[reflect(Resource)]
pub struct HardwareInventory {
    #[reflect(ignore, default = "default_configured_devices")]
    configured:  HashMap<DeviceKey, ConfiguredDevice>,
    #[reflect(ignore, default = "default_configured_device_connections")]
    connections: HashMap<DeviceKey, ConfiguredDeviceConnection>,
}

fn default_configured_devices() -> HashMap<DeviceKey, ConfiguredDevice> { HashMap::new() }

fn default_configured_device_connections() -> HashMap<DeviceKey, ConfiguredDeviceConnection> {
    HashMap::new()
}

impl HardwareInventory {
    /// Retain one authored device without enabling reporters or creating a device entity.
    pub fn configure(&mut self, configured_device: ConfiguredDevice) {
        let device_key = configured_device.key.clone();
        self.configured
            .insert(device_key.clone(), configured_device);
        self.connections
            .entry(device_key)
            .or_insert(ConfiguredDeviceConnection::NotObserved);
    }

    /// Borrow one configured device and its authored operation mode.
    ///
    /// # Errors
    ///
    /// Returns `HardwareInventoryError::DeviceNotConfigured` when no authored entry uses this
    /// durable key.
    pub fn configured_device(
        &self,
        device_key: &DeviceKey,
    ) -> Result<&ConfiguredDevice, HardwareInventoryError> {
        self.configured
            .get(device_key)
            .ok_or_else(|| HardwareInventoryError::DeviceNotConfigured {
                device_key: device_key.clone(),
            })
    }

    /// Read the passive connection conclusion retained for one authored device.
    ///
    /// # Errors
    ///
    /// Returns `HardwareInventoryError::DeviceNotConfigured` for a key that application code did
    /// not author into this inventory.
    pub fn connection(
        &self,
        device_key: &DeviceKey,
    ) -> Result<ConfiguredDeviceConnection, HardwareInventoryError> {
        self.configured_device(device_key)?;
        self.connections.get(device_key).copied().ok_or_else(|| {
            HardwareInventoryError::DeviceNotConfigured {
                device_key: device_key.clone(),
            }
        })
    }

    #[cfg_attr(
        not(test),
        expect(
            dead_code,
            reason = "phase 10 reconciliation updates configured-device connection evidence"
        )
    )]
    pub(crate) fn set_connection(
        &mut self,
        device_key: &DeviceKey,
        connection: ConfiguredDeviceConnection,
    ) -> Result<(), HardwareInventoryError> {
        self.configured_device(device_key)?;
        self.connections.insert(device_key.clone(), connection);
        Ok(())
    }

    #[cfg_attr(
        not(test),
        expect(
            dead_code,
            reason = "phases 10 and 11 block offline capture apply and poll operations"
        )
    )]
    fn ensure_operational(&self, device_key: &DeviceKey) -> Result<(), BindingError> {
        match self.configured.get(device_key) {
            Some(ConfiguredDevice {
                mode: ConfiguredDeviceMode::Offline,
                ..
            }) => Err(BindingError::ConfiguredDeviceOffline {
                device_key: device_key.clone(),
            }),
            Some(ConfiguredDevice {
                mode: ConfiguredDeviceMode::Managed,
                ..
            })
            | None => Ok(()),
        }
    }
}

/// Failure from reading or updating an authored inventory key that does not exist.
#[derive(Clone, Debug, Error, PartialEq, Eq)]
pub enum HardwareInventoryError {
    /// The requested durable key has no authored inventory record in this application.
    #[error("device `{device_key:?}` is not configured")]
    DeviceNotConfigured {
        /// Key that did not select a `ConfiguredDevice` inventory entry.
        device_key: DeviceKey,
    },
}

#[cfg(test)]
mod tests {
    use std::any::TypeId;
    use std::error::Error;
    use std::num::NonZeroUsize;
    use std::sync::Arc;
    use std::sync::Mutex;

    use bevy::app::App;
    use bevy::ecs::reflect::AppTypeRegistry;
    use bevy::prelude::Component;
    use bevy::prelude::Reflect;
    use bevy::prelude::World;

    use super::AvailableConfiguration;
    use super::Binding;
    use super::BindingCapacityError;
    use super::BindingError;
    use super::BindingTransition;
    use super::Bindings;
    use super::ConfiguredDevice;
    use super::ConfiguredDeviceConnection;
    use super::ConfiguredDeviceMode;
    use super::HardwareInventory;
    use super::RequestedConfiguration;
    use super::RetirementOutcome;
    use super::RoleView;
    use crate::ApplyPermit;
    use crate::AttemptId;
    use crate::AttemptOutcome;
    use crate::AttemptProgress;
    use crate::CaptureOutcome;
    use crate::DeviceAccessError;
    use crate::DeviceEndpoint;
    use crate::DeviceIdSource;
    use crate::DeviceKey;
    use crate::DeviceKind;
    use crate::DriverContractError;
    use crate::DriverId;
    use crate::EndpointDriver;
    use crate::EndpointId;
    use crate::LastKnownGoodConfiguration;
    use crate::OnAbort;
    use crate::OnSessionLoss;
    use crate::RecoveryPolicy;
    use crate::RetryOn;
    use crate::RoleKey;
    use crate::RoleState;
    use crate::registration::Drivers;
    use crate::scheme::AuthoredId;

    #[derive(Component, Reflect)]
    struct TestConfiguration(u8);

    struct RecordingDriver {
        applied_configurations: Arc<Mutex<Vec<u8>>>,
    }

    impl EndpointDriver for RecordingDriver {
        type Configuration = TestConfiguration;

        fn capture(
            &mut self,
            _: &mut World,
            _: &DeviceEndpoint,
        ) -> CaptureOutcome<Self::Configuration> {
            CaptureOutcome::Read(TestConfiguration(7))
        }

        fn start_apply(
            &mut self,
            _: &mut World,
            _: &DeviceEndpoint,
            configuration: &Self::Configuration,
            _: AttemptId,
            _: ApplyPermit,
        ) {
            self.applied_configurations
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .push(configuration.0);
        }

        fn poll(&mut self, _: &mut World, _: AttemptId) -> AttemptProgress {
            AttemptProgress::Pending
        }
    }

    #[derive(Debug, Default, PartialEq, Eq)]
    struct DriverCallLog {
        captures:               usize,
        applied_configurations: Vec<u8>,
        polls:                  usize,
    }

    struct CallCountingDriver {
        driver_call_log: Arc<Mutex<DriverCallLog>>,
    }

    impl EndpointDriver for CallCountingDriver {
        type Configuration = TestConfiguration;

        fn capture(
            &mut self,
            _: &mut World,
            _: &DeviceEndpoint,
        ) -> CaptureOutcome<Self::Configuration> {
            self.driver_call_log
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .captures += 1;
            CaptureOutcome::Read(TestConfiguration(7))
        }

        fn start_apply(
            &mut self,
            _: &mut World,
            _: &DeviceEndpoint,
            configuration: &Self::Configuration,
            _: AttemptId,
            _: ApplyPermit,
        ) {
            self.driver_call_log
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .applied_configurations
                .push(configuration.0);
        }

        fn poll(&mut self, _: &mut World, _: AttemptId) -> AttemptProgress {
            self.driver_call_log
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .polls += 1;
            AttemptProgress::Pending
        }
    }

    #[derive(Component, Reflect)]
    struct MismatchedConfiguration;

    struct MismatchedDriver;

    impl EndpointDriver for MismatchedDriver {
        type Configuration = MismatchedConfiguration;

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
            AttemptProgress::Pending
        }
    }

    #[test]
    fn duplicate_role_and_endpoint_registration_preserve_the_first_binding()
    -> Result<(), Box<dyn Error>> {
        let endpoint = display_endpoint("studio-display")?;
        let first_role = RoleKey::new("primary-window")?;
        let second_role = RoleKey::new("secondary-window")?;
        let mut bindings = Bindings::default();
        bindings.register(binding(first_role.clone(), endpoint.clone()))?;

        assert!(matches!(
            bindings.register(binding(first_role.clone(), display_endpoint("other-display")?)),
            Err(BindingError::RoleAlreadyBound { role }) if role == first_role
        ));
        assert!(matches!(
            bindings.register(binding(second_role, endpoint)),
            Err(BindingError::EndpointAlreadyOwned { owner, .. }) if owner == first_role
        ));
        assert_eq!(
            bindings.roles_for(&device_key("studio-display")?).count(),
            1
        );
        assert!(bindings.binding(&first_role).is_ok());

        Ok(())
    }

    #[test]
    fn failed_replace_keeps_each_existing_reverse_index() -> Result<(), Box<dyn Error>> {
        let first_role = RoleKey::new("primary-window")?;
        let second_role = RoleKey::new("secondary-window")?;
        let first_endpoint = display_endpoint("studio-display")?;
        let second_endpoint = display_endpoint("edit-display")?;
        let mut bindings = Bindings::default();
        bindings.register(binding(first_role.clone(), first_endpoint.clone()))?;
        bindings.register(binding(second_role.clone(), second_endpoint.clone()))?;

        assert!(matches!(
            bindings.replace(binding(first_role.clone(), second_endpoint.clone())),
            Err(BindingError::EndpointAlreadyOwned { owner, .. }) if owner == second_role
        ));
        assert_eq!(bindings.binding(&first_role)?.endpoint, first_endpoint);
        assert_eq!(bindings.binding(&second_role)?.endpoint, second_endpoint);
        assert_eq!(
            bindings.roles_for(&device_key("studio-display")?).count(),
            1
        );
        assert_eq!(bindings.roles_for(&device_key("edit-display")?).count(), 1);

        Ok(())
    }

    #[test]
    fn successful_replace_releases_only_its_old_endpoint() -> Result<(), Box<dyn Error>> {
        let role = RoleKey::new("primary-window")?;
        let old_endpoint = display_endpoint("studio-display")?;
        let new_endpoint = display_endpoint("edit-display")?;
        let mut bindings = Bindings::default();
        bindings.register(binding(role.clone(), old_endpoint.clone()))?;

        let displaced = bindings.replace(binding(role.clone(), new_endpoint.clone()))?;

        assert_eq!(displaced.endpoint, old_endpoint);
        assert_eq!(bindings.binding(&role)?.endpoint, new_endpoint);
        assert_eq!(
            bindings.roles_for(&device_key("studio-display")?).count(),
            0
        );
        assert_eq!(bindings.roles_for(&device_key("edit-display")?).count(), 1);

        Ok(())
    }

    #[test]
    fn retirement_is_idempotent_and_removes_every_index() -> Result<(), Box<dyn Error>> {
        let role = RoleKey::new("primary-window")?;
        let endpoint = display_endpoint("studio-display")?;
        let device_key = endpoint.device.clone();
        let mut bindings = Bindings::default();
        bindings.register(binding(role.clone(), endpoint))?;

        let retirement = bindings.retire(&role)?;

        assert!(matches!(
            retirement,
            RetirementOutcome::Retired(Binding {
                state: RoleState::Retired,
                ..
            })
        ));
        assert!(matches!(
            bindings.retire(&role)?,
            RetirementOutcome::AlreadyUnbound
        ));
        assert!(matches!(
            bindings.binding(&role),
            Err(BindingError::RoleNotBound { .. })
        ));
        assert_eq!(bindings.roles_for(&device_key).count(), 0);

        Ok(())
    }

    #[test]
    fn one_device_can_serve_several_roles_at_distinct_endpoints() -> Result<(), Box<dyn Error>> {
        let device_key = device_key("control-panel")?;
        let first_role = RoleKey::new("cut")?;
        let second_role = RoleKey::new("fade")?;
        let mut bindings = Bindings::default();
        bindings.register(binding(
            first_role,
            DeviceEndpoint {
                device: device_key.clone(),
                id:     EndpointId::Part(crate::PartName::new("key/1")?),
            },
        ))?;
        bindings.register(binding(
            second_role,
            DeviceEndpoint {
                device: device_key.clone(),
                id:     EndpointId::Part(crate::PartName::new("key/2")?),
            },
        ))?;

        assert_eq!(bindings.roles_for(&device_key).count(), 2);

        Ok(())
    }

    #[test]
    fn transitions_are_monotonic_and_hold_only_lifecycle_metadata() -> Result<(), Box<dyn Error>> {
        let role = RoleKey::new("primary-window")?;
        let mut bindings = Bindings::default();
        bindings.register(binding(role.clone(), display_endpoint("studio-display")?))?;
        bindings.replace(binding(role.clone(), display_endpoint("edit-display")?))?;
        let _ = bindings.retire(&role)?;
        let _ = bindings.retire(&role)?;

        let transitions = bindings.take_pending_transitions();
        let sequences = transitions
            .iter()
            .map(|binding_transition| match binding_transition {
                BindingTransition::Registered {
                    sequence,
                    role: transition_role,
                }
                | BindingTransition::Replaced {
                    sequence,
                    role: transition_role,
                }
                | BindingTransition::Retired {
                    sequence,
                    role: transition_role,
                } => {
                    assert_eq!(transition_role, &role);
                    sequence.get()
                },
            })
            .collect::<Vec<_>>();

        assert_eq!(sequences, vec![0, 1, 2]);

        Ok(())
    }

    #[test]
    fn configured_transition_capacity_keeps_register_replace_and_retire_atomic()
    -> Result<(), Box<dyn Error>> {
        let first_role = RoleKey::new("primary-window")?;
        let second_role = RoleKey::new("secondary-window")?;
        let third_role = RoleKey::new("tertiary-window")?;
        let first_endpoint = display_endpoint("studio-display")?;
        let second_endpoint = display_endpoint("edit-display")?;
        let third_endpoint = display_endpoint("presentation-display")?;
        let replacement_endpoint = display_endpoint("replacement-display")?;
        let mut bindings = Bindings::default();
        let capacity = NonZeroUsize::new(2).ok_or("nonzero capacity")?;
        bindings.set_pending_transition_capacity(capacity)?;
        bindings.register(binding(first_role.clone(), first_endpoint.clone()))?;
        bindings.register(binding(second_role.clone(), second_endpoint.clone()))?;

        assert_eq!(
            bindings.register(binding(third_role.clone(), third_endpoint.clone())),
            Err(BindingError::PendingTransitionCapacityReached)
        );
        assert!(matches!(
            bindings.replace(binding(first_role.clone(), replacement_endpoint.clone())),
            Err(BindingError::PendingTransitionCapacityReached)
        ));
        assert!(matches!(
            bindings.retire(&first_role),
            Err(BindingError::PendingTransitionCapacityReached)
        ));
        assert_eq!(bindings.binding(&first_role)?.endpoint, first_endpoint);
        assert_eq!(bindings.binding(&second_role)?.endpoint, second_endpoint);
        assert!(matches!(
            bindings.binding(&third_role),
            Err(BindingError::RoleNotBound { .. })
        ));
        assert_eq!(
            bindings.owner_by_endpoint.get(&first_endpoint),
            Some(&first_role)
        );
        assert_eq!(
            bindings.owner_by_endpoint.get(&second_endpoint),
            Some(&second_role)
        );
        assert!(!bindings.owner_by_endpoint.contains_key(&third_endpoint));
        assert!(
            !bindings
                .owner_by_endpoint
                .contains_key(&replacement_endpoint)
        );
        assert_eq!(
            bindings.roles_by_device.get(&first_endpoint.device),
            Some(&vec![first_role.clone()])
        );
        assert_eq!(
            bindings.roles_by_device.get(&second_endpoint.device),
            Some(&vec![second_role])
        );
        assert!(
            !bindings
                .roles_by_device
                .contains_key(&third_endpoint.device)
        );
        assert!(
            !bindings
                .roles_by_device
                .contains_key(&replacement_endpoint.device)
        );
        assert_eq!(
            bindings.set_pending_transition_capacity(NonZeroUsize::MIN),
            Err(BindingCapacityError::BelowPendingCount {
                capacity: NonZeroUsize::MIN,
                pending:  2,
            })
        );

        Ok(())
    }

    #[test]
    fn default_transition_capacity_rejects_another_registration_without_index_mutation()
    -> Result<(), Box<dyn Error>> {
        let mut bindings = Bindings::default();

        for index in 0..super::DEFAULT_PENDING_TRANSITION_CAPACITY {
            let role = RoleKey::new(format!("default-capacity-role-{index}"))?;
            let endpoint = display_endpoint(&format!("default-capacity-device-{index}"))?;
            bindings.register(binding(role, endpoint))?;
        }

        let overflow_role = RoleKey::new("default-capacity-overflow")?;
        let overflow_endpoint = display_endpoint("default-capacity-overflow-device")?;
        assert_eq!(
            bindings.register(binding(overflow_role.clone(), overflow_endpoint.clone())),
            Err(BindingError::PendingTransitionCapacityReached)
        );
        assert!(matches!(
            bindings.binding(&overflow_role),
            Err(BindingError::RoleNotBound { .. })
        ));
        assert!(!bindings.owner_by_endpoint.contains_key(&overflow_endpoint));
        assert!(
            !bindings
                .roles_by_device
                .contains_key(&overflow_endpoint.device)
        );
        assert_eq!(
            bindings.pending_transitions.queue.len(),
            super::DEFAULT_PENDING_TRANSITION_CAPACITY
        );

        Ok(())
    }

    #[test]
    fn transition_sequence_exhaustion_keeps_all_binding_indexes_unchanged()
    -> Result<(), Box<dyn Error>> {
        let first_role = RoleKey::new("last-sequence-role")?;
        let first_endpoint = display_endpoint("last-sequence-device")?;
        let second_role = RoleKey::new("exhausted-sequence-role")?;
        let second_endpoint = display_endpoint("exhausted-sequence-device")?;
        let mut bindings = Bindings {
            next_transition_sequence: u64::MAX - 1,
            ..Default::default()
        };

        bindings.register(binding(first_role.clone(), first_endpoint.clone()))?;
        assert!(matches!(
            bindings.pending_transitions.queue.front(),
            Some(BindingTransition::Registered { sequence, role })
                if sequence.get() == u64::MAX - 1 && role == &first_role
        ));
        assert_eq!(bindings.next_transition_sequence, u64::MAX);

        assert_eq!(
            bindings.register(binding(second_role.clone(), second_endpoint.clone())),
            Err(BindingError::TransitionSequenceExhausted)
        );
        assert_eq!(bindings.binding(&first_role)?.endpoint, first_endpoint);
        assert!(matches!(
            bindings.binding(&second_role),
            Err(BindingError::RoleNotBound { .. })
        ));
        assert_eq!(
            bindings.owner_by_endpoint.get(&first_endpoint),
            Some(&first_role)
        );
        assert!(!bindings.owner_by_endpoint.contains_key(&second_endpoint));
        assert_eq!(
            bindings.roles_by_device.get(&first_endpoint.device),
            Some(&vec![first_role])
        );
        assert!(
            !bindings
                .roles_by_device
                .contains_key(&second_endpoint.device)
        );
        assert_eq!(bindings.next_transition_sequence, u64::MAX);

        Ok(())
    }

    #[test]
    fn in_service_apply_keeps_requested_and_readback_configuration_distinct()
    -> Result<(), Box<dyn Error>> {
        let role = RoleKey::new("primary-window")?;
        let mut bindings = Bindings::default();
        let hardware_inventory = HardwareInventory::default();
        let applied_configurations = Arc::new(Mutex::new(Vec::new()));
        let mut drivers = Drivers::default();
        let driver = drivers.add(RecordingDriver {
            applied_configurations: Arc::clone(&applied_configurations),
        });
        assert_eq!(driver, DriverId(0));
        bindings.register(binding(role.clone(), display_endpoint("studio-display")?))?;

        assert!(matches!(bindings.role_view(&role)?, RoleView::Waiting(_)));
        assert!(matches!(
            match bindings.role_view(&role)? {
                RoleView::Waiting(waiting_role) => waiting_role.start_requested_apply(
                    AttemptId::default(),
                    ApplyPermit::restore_only(),
                    &hardware_inventory,
                ),
                _ => return Err("new binding must select waiting view".into()),
            },
            Err(BindingError::RequestedConfigurationRequiresInServicePermit)
        ));
        {
            let apply_request = match bindings.role_view(&role)? {
                RoleView::Waiting(waiting_role) => waiting_role.start_requested_apply(
                    AttemptId::default(),
                    ApplyPermit::in_service(),
                    &hardware_inventory,
                )?,
                _ => return Err("new binding must select waiting view".into()),
            };
            assert_eq!(
                drivers.start_apply(&mut World::new(), apply_request),
                Ok(())
            );
        }
        assert_eq!(
            *applied_configurations
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner),
            vec![3]
        );
        match bindings.role_view(&role)? {
            RoleView::Applying(mut applying_role) => {
                applying_role.finish(AttemptOutcome::Succeeded);
            },
            _ => return Err("apply request must select applying view".into()),
        }
        match bindings.role_view(&role)? {
            RoleView::Ready(mut ready_role) => {
                ready_role.record_capture(CaptureOutcome::Read(LastKnownGoodConfiguration::known(
                    TestConfiguration(7),
                )));
            },
            _ => return Err("successful apply must select ready view".into()),
        }

        match bindings.configuration_for(&role)? {
            AvailableConfiguration::LastKnownGood(configuration) => {
                assert_eq!(
                    configuration
                        .as_any()
                        .downcast_ref::<TestConfiguration>()
                        .map(|test_configuration| test_configuration.0),
                    Some(7)
                );
            },
            AvailableConfiguration::Requested(_) => {
                return Err("safe readback must take precedence over requested intent".into());
            },
        }

        Ok(())
    }

    #[test]
    fn restore_only_apply_uses_last_known_good_configuration() -> Result<(), Box<dyn Error>> {
        let role = RoleKey::new("primary-window")?;
        let hardware_inventory = HardwareInventory::default();
        let applied_configurations = Arc::new(Mutex::new(Vec::new()));
        let mut drivers = Drivers::default();
        let driver = drivers.add(RecordingDriver {
            applied_configurations: Arc::clone(&applied_configurations),
        });
        let mut configured_binding = binding(role.clone(), display_endpoint("studio-display")?);
        configured_binding.driver = driver;
        configured_binding.last_known_good =
            LastKnownGoodConfiguration::known(TestConfiguration(7));
        let mut bindings = Bindings::default();
        bindings.register(configured_binding)?;

        assert!(matches!(
            match bindings.role_view(&role)? {
                RoleView::Waiting(waiting_role) => waiting_role.start_last_known_good_restore(
                    AttemptId::default(),
                    ApplyPermit::in_service(),
                    &hardware_inventory,
                ),
                _ => return Err("registered binding must select waiting view".into()),
            },
            Err(BindingError::LastKnownGoodConfigurationRequiresRestoreOnlyPermit)
        ));

        let apply_request = match bindings.role_view(&role)? {
            RoleView::Waiting(waiting_role) => waiting_role.start_last_known_good_restore(
                AttemptId::default(),
                ApplyPermit::restore_only(),
                &hardware_inventory,
            )?,
            _ => return Err("restore authorization failure must retain waiting state".into()),
        };
        assert_eq!(
            drivers.start_apply(&mut World::new(), apply_request),
            Ok(())
        );
        assert_eq!(
            *applied_configurations
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner),
            vec![7]
        );
        assert!(matches!(bindings.role_view(&role)?, RoleView::Applying(_)));

        Ok(())
    }

    #[test]
    fn dropped_start_apply_request_leaves_the_binding_waiting() -> Result<(), Box<dyn Error>> {
        let role = RoleKey::new("primary-window")?;
        let hardware_inventory = HardwareInventory::default();
        let mut bindings = Bindings::default();
        bindings.register(binding(role.clone(), display_endpoint("studio-display")?))?;

        let _ = match bindings.role_view(&role)? {
            RoleView::Waiting(waiting_role) => waiting_role.start_requested_apply(
                AttemptId::default(),
                ApplyPermit::in_service(),
                &hardware_inventory,
            )?,
            _ => return Err("registered binding must select waiting view".into()),
        };

        assert!(matches!(bindings.role_view(&role)?, RoleView::Waiting(_)));

        Ok(())
    }

    #[test]
    fn unregistered_driver_dispatch_leaves_the_binding_waiting() -> Result<(), Box<dyn Error>> {
        let role = RoleKey::new("primary-window")?;
        let hardware_inventory = HardwareInventory::default();
        let mut bindings = Bindings::default();
        bindings.register(binding(role.clone(), display_endpoint("studio-display")?))?;
        let apply_request = match bindings.role_view(&role)? {
            RoleView::Waiting(waiting_role) => waiting_role.start_requested_apply(
                AttemptId::default(),
                ApplyPermit::in_service(),
                &hardware_inventory,
            )?,
            _ => return Err("registered binding must select waiting view".into()),
        };

        let start_apply_result = Drivers::default().start_apply(&mut World::new(), apply_request);

        assert!(matches!(
            start_apply_result,
            Err(DriverContractError::DriverNotRegistered { .. })
        ));
        assert!(matches!(bindings.role_view(&role)?, RoleView::Waiting(_)));

        Ok(())
    }

    #[test]
    fn type_mismatch_dispatch_leaves_the_binding_waiting() -> Result<(), Box<dyn Error>> {
        let role = RoleKey::new("primary-window")?;
        let hardware_inventory = HardwareInventory::default();
        let mut bindings = Bindings::default();
        let mut drivers = Drivers::default();
        let driver = drivers.add(MismatchedDriver);
        let mut configured_binding = binding(role.clone(), display_endpoint("studio-display")?);
        configured_binding.driver = driver;
        bindings.register(configured_binding)?;
        let apply_request = match bindings.role_view(&role)? {
            RoleView::Waiting(waiting_role) => waiting_role.start_requested_apply(
                AttemptId::default(),
                ApplyPermit::in_service(),
                &hardware_inventory,
            )?,
            _ => return Err("registered binding must select waiting view".into()),
        };

        let start_apply_result = drivers.start_apply(&mut World::new(), apply_request);

        assert!(matches!(
            start_apply_result,
            Err(DriverContractError::ConfigurationTypeMismatch { .. })
        ));
        assert!(matches!(bindings.role_view(&role)?, RoleView::Waiting(_)));

        Ok(())
    }

    #[test]
    fn substituted_apply_returns_the_role_to_ready_for_safe_readback() -> Result<(), Box<dyn Error>>
    {
        let role = RoleKey::new("primary-window")?;
        let hardware_inventory = HardwareInventory::default();
        let mut drivers = Drivers::default();
        let driver = drivers.add(RecordingDriver {
            applied_configurations: Arc::new(Mutex::new(Vec::new())),
        });
        let mut configured_binding = binding(role.clone(), display_endpoint("studio-display")?);
        configured_binding.driver = driver;
        let mut bindings = Bindings::default();
        bindings.register(configured_binding)?;
        let apply_request = match bindings.role_view(&role)? {
            RoleView::Waiting(waiting_role) => waiting_role.start_requested_apply(
                AttemptId::default(),
                ApplyPermit::in_service(),
                &hardware_inventory,
            )?,
            _ => return Err("registered binding must select waiting view".into()),
        };
        drivers.start_apply(&mut World::new(), apply_request)?;

        match bindings.role_view(&role)? {
            RoleView::Applying(mut applying_role) => {
                applying_role.finish(AttemptOutcome::Substituted);
            },
            _ => return Err("dispatched apply must select applying view".into()),
        }

        assert!(matches!(bindings.role_view(&role)?, RoleView::Ready(_)));

        Ok(())
    }

    #[test]
    fn aborting_an_dispatched_apply_returns_the_role_to_waiting() -> Result<(), Box<dyn Error>> {
        let role = RoleKey::new("primary-window")?;
        let hardware_inventory = HardwareInventory::default();
        let mut drivers = Drivers::default();
        let driver = drivers.add(RecordingDriver {
            applied_configurations: Arc::new(Mutex::new(Vec::new())),
        });
        let mut configured_binding = binding(role.clone(), display_endpoint("studio-display")?);
        configured_binding.driver = driver;
        let mut bindings = Bindings::default();
        bindings.register(configured_binding)?;
        let apply_request = match bindings.role_view(&role)? {
            RoleView::Waiting(waiting_role) => waiting_role.start_requested_apply(
                AttemptId::default(),
                ApplyPermit::in_service(),
                &hardware_inventory,
            )?,
            _ => return Err("registered binding must select waiting view".into()),
        };
        drivers.start_apply(&mut World::new(), apply_request)?;

        match bindings.role_view(&role)? {
            RoleView::Applying(mut applying_role) => applying_role.abort(),
            _ => return Err("dispatched apply must select applying view".into()),
        }

        assert!(matches!(bindings.role_view(&role)?, RoleView::Waiting(_)));

        Ok(())
    }

    #[test]
    fn read_failure_retains_prior_value_and_not_readable_stops_future_requests()
    -> Result<(), Box<dyn Error>> {
        let role = RoleKey::new("primary-window")?;
        let mut bindings = Bindings::default();
        let hardware_inventory = HardwareInventory::default();
        let mut drivers = Drivers::default();
        let driver = drivers.add(RecordingDriver {
            applied_configurations: Arc::new(Mutex::new(Vec::new())),
        });
        assert_eq!(driver, DriverId(0));
        let mut configured_binding = binding(role.clone(), display_endpoint("studio-display")?);
        configured_binding.state = RoleState::Ready;
        configured_binding.last_known_good =
            LastKnownGoodConfiguration::known(TestConfiguration(7));
        bindings.register(configured_binding)?;

        match bindings.role_view(&role)? {
            RoleView::Waiting(waiting_role) => {
                let apply_request = waiting_role.start_requested_apply(
                    AttemptId::default(),
                    ApplyPermit::in_service(),
                    &hardware_inventory,
                )?;
                assert_eq!(
                    drivers.start_apply(&mut World::new(), apply_request),
                    Ok(())
                );
            },
            _ => return Err("registration resets role state to waiting".into()),
        }
        match bindings.role_view(&role)? {
            RoleView::Applying(mut applying_role) => {
                applying_role.finish(AttemptOutcome::Succeeded);
            },
            _ => return Err("requested operation must select applying view".into()),
        }
        match bindings.role_view(&role)? {
            RoleView::Ready(mut ready_role) => {
                ready_role.record_capture(CaptureOutcome::ReadFailed(DeviceAccessError::Absent {
                    detail: String::from("test departure"),
                }));
                ready_role.record_capture(CaptureOutcome::NotReadable);
            },
            _ => return Err("successful operation must select ready view".into()),
        }

        match bindings.configuration_for(&role)? {
            AvailableConfiguration::LastKnownGood(configuration) => assert_eq!(
                configuration
                    .as_any()
                    .downcast_ref::<TestConfiguration>()
                    .map(|test_configuration| test_configuration.0),
                Some(7)
            ),
            AvailableConfiguration::Requested(_) => {
                return Err("read failure must retain prior known value".into());
            },
        }
        match bindings.role_view(&role)? {
            RoleView::Ready(ready_role) => assert!(matches!(
                ready_role.capture_request(&hardware_inventory),
                Err(BindingError::ConfigurationNotReadable { .. })
            )),
            _ => return Err("readability test requires ready view".into()),
        }

        Ok(())
    }

    #[test]
    fn offline_waiting_role_cannot_mint_apply_requests() -> Result<(), Box<dyn Error>> {
        let role = RoleKey::new("primary-window")?;
        let endpoint = display_endpoint("studio-display")?;
        let driver_call_log = Arc::new(Mutex::new(DriverCallLog::default()));
        let mut drivers = Drivers::default();
        let driver = drivers.add(CallCountingDriver {
            driver_call_log: Arc::clone(&driver_call_log),
        });
        let mut bindings = Bindings::default();
        let mut configured_binding = binding(role.clone(), endpoint.clone());
        configured_binding.driver = driver;
        bindings.register(configured_binding)?;
        let mut hardware_inventory = HardwareInventory::default();
        hardware_inventory.configure(ConfiguredDevice {
            key:  endpoint.device.clone(),
            mode: ConfiguredDeviceMode::Offline,
        });

        match bindings.configuration_for(&role)? {
            AvailableConfiguration::Requested(configuration) => assert_eq!(
                configuration
                    .as_any()
                    .downcast_ref::<TestConfiguration>()
                    .map(|test_configuration| test_configuration.0),
                Some(3)
            ),
            AvailableConfiguration::LastKnownGood(_) => {
                return Err("no safe readback has established a configuration".into());
            },
        }
        assert_eq!(
            hardware_inventory.connection(&endpoint.device)?,
            ConfiguredDeviceConnection::NotObserved
        );
        hardware_inventory.set_connection(&endpoint.device, ConfiguredDeviceConnection::Present)?;
        assert_eq!(
            hardware_inventory.connection(&endpoint.device)?,
            ConfiguredDeviceConnection::Present
        );
        match bindings.role_view(&role)? {
            RoleView::Waiting(waiting_role) => assert!(matches!(
                waiting_role.start_requested_apply(
                    AttemptId::default(),
                    ApplyPermit::in_service(),
                    &hardware_inventory,
                ),
                Err(BindingError::ConfiguredDeviceOffline { .. })
            )),
            _ => return Err("registered offline binding must remain waiting".into()),
        }
        assert!(matches!(bindings.role_view(&role)?, RoleView::Waiting(_)));
        match bindings.role_view(&role)? {
            RoleView::Waiting(waiting_role) => assert!(matches!(
                waiting_role.start_last_known_good_restore(
                    AttemptId::default(),
                    ApplyPermit::restore_only(),
                    &hardware_inventory,
                ),
                Err(BindingError::ConfiguredDeviceOffline { .. })
            )),
            _ => return Err("offline requested-apply refusal must retain waiting state".into()),
        }
        assert!(matches!(bindings.role_view(&role)?, RoleView::Waiting(_)));
        assert_eq!(
            *driver_call_log
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner),
            DriverCallLog::default()
        );

        Ok(())
    }

    #[test]
    fn offline_inventory_blocks_ready_capture_and_applying_poll_requests()
    -> Result<(), Box<dyn Error>> {
        let role = RoleKey::new("primary-window")?;
        let endpoint = display_endpoint("studio-display")?;
        let driver_call_log = Arc::new(Mutex::new(DriverCallLog::default()));
        let mut drivers = Drivers::default();
        let driver = drivers.add(CallCountingDriver {
            driver_call_log: Arc::clone(&driver_call_log),
        });
        let mut bindings = Bindings::default();
        let mut configured_binding = binding(role.clone(), endpoint.clone());
        configured_binding.driver = driver;
        bindings.register(configured_binding)?;
        let mut hardware_inventory = HardwareInventory::default();
        hardware_inventory.configure(ConfiguredDevice {
            key:  endpoint.device.clone(),
            mode: ConfiguredDeviceMode::Managed,
        });
        let start_apply_request = match bindings.role_view(&role)? {
            RoleView::Waiting(waiting_role) => waiting_role.start_requested_apply(
                AttemptId::default(),
                ApplyPermit::in_service(),
                &hardware_inventory,
            )?,
            _ => return Err("managed binding must select waiting state before apply".into()),
        };
        drivers.start_apply(&mut World::new(), start_apply_request)?;
        assert!(matches!(bindings.role_view(&role)?, RoleView::Applying(_)));

        hardware_inventory.configure(ConfiguredDevice {
            key:  endpoint.device.clone(),
            mode: ConfiguredDeviceMode::Offline,
        });
        match bindings.role_view(&role)? {
            RoleView::Applying(applying_role) => assert!(matches!(
                applying_role.poll_request(&hardware_inventory),
                Err(BindingError::ConfiguredDeviceOffline { .. })
            )),
            _ => return Err("started apply must select applying state".into()),
        }
        assert!(matches!(bindings.role_view(&role)?, RoleView::Applying(_)));
        assert_eq!(
            *driver_call_log
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner),
            DriverCallLog {
                captures:               0,
                applied_configurations: vec![3],
                polls:                  0,
            }
        );

        hardware_inventory.configure(ConfiguredDevice {
            key:  endpoint.device.clone(),
            mode: ConfiguredDeviceMode::Managed,
        });
        match bindings.role_view(&role)? {
            RoleView::Applying(mut applying_role) => {
                applying_role.finish(AttemptOutcome::Succeeded);
            },
            _ => return Err("applying role must remain finishable after poll refusal".into()),
        }
        assert!(matches!(bindings.role_view(&role)?, RoleView::Ready(_)));

        hardware_inventory.configure(ConfiguredDevice {
            key:  endpoint.device,
            mode: ConfiguredDeviceMode::Offline,
        });
        match bindings.role_view(&role)? {
            RoleView::Ready(ready_role) => assert!(matches!(
                ready_role.capture_request(&hardware_inventory),
                Err(BindingError::ConfiguredDeviceOffline { .. })
            )),
            _ => return Err("successful apply must select ready state".into()),
        }
        assert!(matches!(bindings.role_view(&role)?, RoleView::Ready(_)));
        assert_eq!(
            *driver_call_log
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner),
            DriverCallLog {
                captures:               0,
                applied_configurations: vec![3],
                polls:                  0,
            }
        );

        Ok(())
    }

    #[test]
    fn binding_inventory_and_reflected_configuration_types_register_automatically() {
        let app = App::new();
        let world = app.world();
        let type_registry = world.resource::<AppTypeRegistry>().read();

        for type_id in [
            TypeId::of::<Bindings>(),
            TypeId::of::<HardwareInventory>(),
            TypeId::of::<Binding>(),
            TypeId::of::<ConfiguredDevice>(),
        ] {
            assert!(type_registry.contains(type_id));
        }
        drop(type_registry);
    }

    fn binding(role: RoleKey, endpoint: DeviceEndpoint) -> Binding {
        Binding {
            role,
            endpoint,
            driver: DriverId(0),
            recovery: RecoveryPolicy::default(),
            retry: RetryOn::NewRevision,
            on_abort: OnAbort::default(),
            on_loss: OnSessionLoss::default(),
            state: RoleState::Ready,
            requested: RequestedConfiguration::new(TestConfiguration(3)),
            last_known_good: LastKnownGoodConfiguration::default(),
        }
    }

    fn display_endpoint(value: &str) -> Result<DeviceEndpoint, Box<dyn Error>> {
        Ok(DeviceEndpoint {
            device: device_key(value)?,
            id:     EndpointId::Whole,
        })
    }

    fn device_key(value: &str) -> Result<DeviceKey, Box<dyn Error>> {
        Ok(DeviceKey {
            kind: DeviceKind::Display,
            id:   DeviceIdSource::Authored {
                value: AuthoredId::new(value)?,
            },
        })
    }
}
