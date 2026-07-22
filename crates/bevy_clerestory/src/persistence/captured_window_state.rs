//! Authoritative captured window placement and persistence state.

use std::collections::HashMap;

use bevy::prelude::*;
use bevy::window::PrimaryWindow;
use bevy_kana::ToI32;
use bevy_kana::ToU32;

use super::PersistedWindowState;
use super::SavedWindowMode;
use super::WindowKey;
use super::save;
use crate::ManagedWindowPersistence;
use crate::Platform;
use crate::monitors::CurrentMonitor;
use crate::monitors::MonitorInfo;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum CapturedWindowPosition {
    Restorable { logical_offset: IVec2 },
    CompositorControlled,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum RebasedCapturedPosition {
    Restorable {
        physical_position: IVec2,
        logical_position:  IVec2,
    },
    CompositorControlled,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct CapturedWindowPlacement {
    pub(crate) monitor_snapshot:  MonitorInfo,
    pub(crate) position:          CapturedWindowPosition,
    pub(crate) logical_size:      UVec2,
    pub(crate) saved_window_mode: SavedWindowMode,
    pub(crate) captured_scale:    f64,
}

impl CapturedWindowPlacement {
    pub(crate) fn capture(
        window: &Window,
        current_monitor: &CurrentMonitor,
        physical_position: Option<IVec2>,
        platform: Platform,
    ) -> Self {
        let monitor_snapshot = current_monitor.monitor_info;
        let position = if platform.position_available() {
            physical_position.map_or(CapturedWindowPosition::CompositorControlled, |position| {
                let physical_offset = position - monitor_snapshot.physical_position;
                CapturedWindowPosition::Restorable {
                    logical_offset: IVec2::new(
                        (f64::from(physical_offset.x) / monitor_snapshot.scale)
                            .round()
                            .to_i32(),
                        (f64::from(physical_offset.y) / monitor_snapshot.scale)
                            .round()
                            .to_i32(),
                    ),
                }
            })
        } else {
            CapturedWindowPosition::CompositorControlled
        };

        Self {
            monitor_snapshot,
            position,
            logical_size: UVec2::new(
                window.resolution.width().to_u32(),
                window.resolution.height().to_u32(),
            ),
            saved_window_mode: (&current_monitor.effective_window_mode).into(),
            captured_scale: monitor_snapshot.scale,
        }
    }

    #[must_use]
    pub(crate) fn rebased_position(&self, live_monitor: &MonitorInfo) -> RebasedCapturedPosition {
        match self.position {
            CapturedWindowPosition::Restorable { logical_offset } => {
                RebasedCapturedPosition::Restorable {
                    physical_position: IVec2::new(
                        live_monitor.physical_position.x
                            + (f64::from(logical_offset.x) * live_monitor.scale)
                                .round()
                                .to_i32(),
                        live_monitor.physical_position.y
                            + (f64::from(logical_offset.y) * live_monitor.scale)
                                .round()
                                .to_i32(),
                    ),
                    logical_position:  self.persisted_logical_position(logical_offset),
                }
            },
            CapturedWindowPosition::CompositorControlled => {
                RebasedCapturedPosition::CompositorControlled
            },
        }
    }

    #[cfg(test)]
    #[must_use]
    pub(crate) fn rebased_physical_position(&self, live_monitor: &MonitorInfo) -> Option<IVec2> {
        match self.rebased_position(live_monitor) {
            RebasedCapturedPosition::Restorable {
                physical_position, ..
            } => Some(physical_position),
            RebasedCapturedPosition::CompositorControlled => None,
        }
    }

    fn persisted_logical_position(&self, logical_offset: IVec2) -> IVec2 {
        let logical_monitor_origin = IVec2::new(
            (f64::from(self.monitor_snapshot.physical_position.x) / self.captured_scale)
                .round()
                .to_i32(),
            (f64::from(self.monitor_snapshot.physical_position.y) / self.captured_scale)
                .round()
                .to_i32(),
        );
        logical_monitor_origin + logical_offset
    }

    pub(crate) fn project(&self, app_name: &str) -> PersistedWindowState {
        let logical_position = match self.position {
            CapturedWindowPosition::Restorable { logical_offset } => {
                let adapter_position = self.persisted_logical_position(logical_offset);
                Some((adapter_position.x, adapter_position.y))
            },
            CapturedWindowPosition::CompositorControlled => None,
        };

        PersistedWindowState {
            logical_position,
            logical_width: self.logical_size.x,
            logical_height: self.logical_size.y,
            scale: self.captured_scale,
            monitor: self.monitor_snapshot.index,
            saved_window_mode: self.saved_window_mode.clone(),
            app_name: app_name.to_string(),
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum CapturedPlacement {
    PersistedOnly(PersistedWindowState),
    Captured(CapturedWindowPlacement),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum PersistenceWriteState {
    Writable,
    Frozen,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct LiveWindow {
    pub(crate) entity: Entity,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct CapturedWindowState {
    pub(crate) placement:   CapturedPlacement,
    pub(crate) persistence: PersistenceWriteState,
    pub(crate) live:        Option<LiveWindow>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum DirtyState {
    #[default]
    Clean,
    Dirty,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum StartupLoadState {
    #[default]
    Unread,
    Read,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum StateMutation {
    Unchanged,
    Changed,
}

#[cfg(test)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct PersistenceActivity {
    pub(crate) file_reads:   usize,
    pub(crate) window_scans: usize,
    pub(crate) captures:     usize,
    pub(crate) projections:  usize,
    pub(crate) writes:       usize,
}

#[derive(Default, Resource)]
pub(crate) struct CapturedWindowStates {
    entries:      HashMap<WindowKey, CapturedWindowState>,
    dirty:        DirtyState,
    startup_load: StartupLoadState,
    #[cfg(test)]
    activity:     PersistenceActivity,
}

impl CapturedWindowStates {
    pub(crate) fn seed(&mut self, states: HashMap<WindowKey, PersistedWindowState>) {
        if self.startup_load == StartupLoadState::Read {
            return;
        }
        self.entries = states
            .into_iter()
            .map(|(window_key, persisted)| {
                (
                    window_key,
                    CapturedWindowState {
                        placement:   CapturedPlacement::PersistedOnly(persisted),
                        persistence: PersistenceWriteState::Writable,
                        live:        None,
                    },
                )
            })
            .collect();
        self.startup_load = StartupLoadState::Read;
    }

    #[must_use]
    pub(crate) const fn startup_was_read(&self) -> bool {
        matches!(self.startup_load, StartupLoadState::Read)
    }

    #[must_use]
    #[cfg(test)]
    pub(crate) fn persisted(&self, window_key: &WindowKey) -> Option<&PersistedWindowState> {
        let entry = self.entries.get(window_key)?;
        match &entry.placement {
            CapturedPlacement::PersistedOnly(persisted) => Some(persisted),
            CapturedPlacement::Captured(_) => None,
        }
    }

    #[must_use]
    #[cfg(test)]
    pub(crate) fn restore_state(&self, window_key: &WindowKey) -> Option<PersistedWindowState> {
        let entry = self.entries.get(window_key)?;
        Some(match &entry.placement {
            CapturedPlacement::PersistedOnly(persisted) => persisted.clone(),
            CapturedPlacement::Captured(captured) => captured.project(""),
        })
    }

    #[cfg(test)]
    pub(crate) fn bind(&mut self, window_key: &WindowKey, entity: Entity) {
        if let Some(entry) = self.entries.get_mut(window_key) {
            entry.live = Some(LiveWindow { entity });
        }
    }

    pub(crate) fn bind_and_freeze(&mut self, window_key: &WindowKey, entity: Entity) -> bool {
        let Some(entry) = self.entries.get_mut(window_key) else {
            return false;
        };
        entry.live = Some(LiveWindow { entity });
        entry.persistence = PersistenceWriteState::Frozen;
        true
    }

    #[must_use]
    pub(crate) fn is_bound_to(&self, window_key: &WindowKey, entity: Entity) -> bool {
        self.entries.get(window_key).and_then(|entry| entry.live) == Some(LiveWindow { entity })
    }

    #[cfg(test)]
    pub(crate) fn unbind(&mut self, window_key: &WindowKey, entity: Entity) {
        if let Some(entry) = self.entries.get_mut(window_key)
            && entry.live == Some(LiveWindow { entity })
        {
            entry.live = None;
        }
    }

    pub(crate) fn freeze(&mut self, window_key: &WindowKey) {
        if let Some(entry) = self.entries.get_mut(window_key) {
            entry.persistence = PersistenceWriteState::Frozen;
        }
    }

    pub(crate) fn cancel(
        &mut self,
        window_key: &WindowKey,
        entity: Option<Entity>,
        managed_window_persistence: &ManagedWindowPersistence,
    ) -> StateMutation {
        let Some(entry) = self.entries.get_mut(window_key) else {
            return StateMutation::Unchanged;
        };
        entry.persistence = PersistenceWriteState::Writable;
        if entity.is_some_and(|entity| entry.live == Some(LiveWindow { entity })) {
            return StateMutation::Unchanged;
        }
        self.deactivate_absent(window_key, managed_window_persistence)
    }

    pub(crate) fn deactivate(
        &mut self,
        window_key: &WindowKey,
        entity: Entity,
        managed_window_persistence: &ManagedWindowPersistence,
    ) -> StateMutation {
        let Some(entry) = self.entries.get_mut(window_key) else {
            return StateMutation::Unchanged;
        };
        if entry.live != Some(LiveWindow { entity }) {
            return StateMutation::Unchanged;
        }
        entry.live = None;
        self.deactivate_absent(window_key, managed_window_persistence)
    }

    fn deactivate_absent(
        &mut self,
        window_key: &WindowKey,
        managed_window_persistence: &ManagedWindowPersistence,
    ) -> StateMutation {
        let Some(entry) = self.entries.get_mut(window_key) else {
            return StateMutation::Unchanged;
        };
        if entry.persistence != PersistenceWriteState::Writable {
            return StateMutation::Unchanged;
        }
        match managed_window_persistence {
            ManagedWindowPersistence::RememberAll => {
                let CapturedPlacement::Captured(captured) = &entry.placement else {
                    return StateMutation::Unchanged;
                };
                entry.placement =
                    CapturedPlacement::PersistedOnly(captured.project(&save::application_name()));
            },
            ManagedWindowPersistence::ActiveOnly => {
                self.entries.remove(window_key);
            },
        }
        self.dirty = DirtyState::Dirty;
        StateMutation::Changed
    }

    pub(crate) fn deactivate_entity(
        &mut self,
        entity: Entity,
        managed_window_persistence: &ManagedWindowPersistence,
    ) -> StateMutation {
        let window_keys: Vec<_> = self
            .entries
            .iter()
            .filter(|(_, entry)| entry.live == Some(LiveWindow { entity }))
            .map(|(window_key, _)| window_key.clone())
            .collect();
        window_keys
            .iter()
            .fold(StateMutation::Unchanged, |mutation, window_key| {
                match (
                    mutation,
                    self.deactivate(window_key, entity, managed_window_persistence),
                ) {
                    (StateMutation::Changed, _) | (_, StateMutation::Changed) => {
                        StateMutation::Changed
                    },
                    _ => StateMutation::Unchanged,
                }
            })
    }

    pub(crate) fn capture(
        &mut self,
        window_key: WindowKey,
        entity: Entity,
        placement: CapturedWindowPlacement,
    ) -> StateMutation {
        #[cfg(test)]
        {
            self.activity.captures += 1;
        }

        if let Some(entry) = self.entries.get_mut(&window_key) {
            entry.live = Some(LiveWindow { entity });
            match entry.persistence {
                PersistenceWriteState::Writable => {},
                PersistenceWriteState::Frozen => return StateMutation::Unchanged,
            }
            if entry.placement == CapturedPlacement::Captured(placement.clone()) {
                return StateMutation::Unchanged;
            }
            entry.placement = CapturedPlacement::Captured(placement);
        } else {
            self.entries.insert(
                window_key,
                CapturedWindowState {
                    placement:   CapturedPlacement::Captured(placement),
                    persistence: PersistenceWriteState::Writable,
                    live:        Some(LiveWindow { entity }),
                },
            );
        }

        self.dirty = DirtyState::Dirty;
        StateMutation::Changed
    }

    pub(crate) fn promote(
        &mut self,
        window_key: WindowKey,
        entity: Entity,
        placement: CapturedWindowPlacement,
    ) -> StateMutation {
        let captured = CapturedPlacement::Captured(placement);
        let changed = self.entries.get(&window_key).is_none_or(|entry| {
            entry.placement != captured || entry.persistence != PersistenceWriteState::Writable
        });
        self.entries.insert(
            window_key,
            CapturedWindowState {
                placement:   captured,
                persistence: PersistenceWriteState::Writable,
                live:        Some(LiveWindow { entity }),
            },
        );
        if changed {
            self.dirty = DirtyState::Dirty;
            StateMutation::Changed
        } else {
            StateMutation::Unchanged
        }
    }

    pub(crate) fn apply_policy(
        &mut self,
        managed_window_persistence: &ManagedWindowPersistence,
    ) -> StateMutation {
        if matches!(
            managed_window_persistence,
            ManagedWindowPersistence::RememberAll
        ) {
            return StateMutation::Unchanged;
        }

        let previous_len = self.entries.len();
        self.entries.retain(|_, entry| {
            entry.live.is_some() || entry.persistence != PersistenceWriteState::Writable
        });
        if self.entries.len() == previous_len {
            StateMutation::Unchanged
        } else {
            self.dirty = DirtyState::Dirty;
            StateMutation::Changed
        }
    }

    #[must_use]
    pub(crate) const fn is_dirty(&self) -> bool { matches!(self.dirty, DirtyState::Dirty) }

    pub(crate) const fn mark_clean(&mut self) { self.dirty = DirtyState::Clean; }

    pub(crate) fn project(&self, app_name: &str) -> HashMap<WindowKey, PersistedWindowState> {
        self.entries
            .iter()
            .map(|(window_key, entry)| {
                let persisted = match &entry.placement {
                    CapturedPlacement::PersistedOnly(persisted) => persisted.clone(),
                    CapturedPlacement::Captured(captured) => captured.project(app_name),
                };
                (window_key.clone(), persisted)
            })
            .collect()
    }

    #[cfg(test)]
    pub(crate) const fn record_file_read(&mut self) { self.activity.file_reads += 1; }

    #[cfg(test)]
    pub(crate) const fn record_window_scan(&mut self) { self.activity.window_scans += 1; }

    #[cfg(test)]
    pub(crate) const fn record_projection(&mut self) { self.activity.projections += 1; }

    #[cfg(test)]
    pub(crate) const fn record_write(&mut self) { self.activity.writes += 1; }

    #[cfg(test)]
    pub(crate) const fn activity(&self) -> PersistenceActivity { self.activity }

    #[cfg(test)]
    pub(crate) fn reset_activity(&mut self) { self.activity = PersistenceActivity::default(); }

    #[cfg(test)]
    pub(crate) fn entry(&self, window_key: &WindowKey) -> Option<&CapturedWindowState> {
        self.entries.get(window_key)
    }

    #[cfg(test)]
    pub(crate) fn live_entity(&self, window_key: &WindowKey) -> Option<Entity> {
        self.entries
            .get(window_key)
            .and_then(|entry| entry.live)
            .map(|live| live.entity)
    }

    pub(crate) fn captured_placement(
        &self,
        window_key: &WindowKey,
    ) -> Option<&CapturedWindowPlacement> {
        let entry = self.entries.get(window_key)?;
        match &entry.placement {
            CapturedPlacement::PersistedOnly(_) => None,
            CapturedPlacement::Captured(placement) => Some(placement),
        }
    }

    #[must_use]
    pub(crate) fn placement(&self, window_key: &WindowKey) -> Option<&CapturedPlacement> {
        self.entries.get(window_key).map(|entry| &entry.placement)
    }
}

pub(crate) fn on_primary_window_removed(
    removed: On<Remove, PrimaryWindow>,
    managed_window_persistence: Res<ManagedWindowPersistence>,
    mut captured_window_states: ResMut<CapturedWindowStates>,
) {
    captured_window_states.deactivate(
        &WindowKey::Primary,
        removed.entity,
        &managed_window_persistence,
    );
    captured_window_states.apply_policy(&managed_window_persistence);
}

pub(crate) fn on_window_removed(
    removed: On<Remove, Window>,
    managed_window_persistence: Res<ManagedWindowPersistence>,
    mut captured_window_states: ResMut<CapturedWindowStates>,
) {
    captured_window_states.deactivate_entity(removed.entity, &managed_window_persistence);
    captured_window_states.apply_policy(&managed_window_persistence);
}

#[cfg(test)]
mod tests {
    use std::fs;

    use bevy::window::WindowMode;
    use tempfile::NamedTempFile;

    use super::*;
    use crate::monitors::MonitorIdentity;
    use crate::persistence::format;
    use crate::persistence::save;
    use crate::persistence::save::StateFileWrite;

    const CANCELLED_ORIGINAL_OFFSET: IVec2 = IVec2::new(10, 20);
    const CANCELLED_REPLACEMENT_OFFSET: IVec2 = IVec2::new(30, 40);

    fn monitor(index: usize, scale: f64, physical_position: IVec2) -> MonitorInfo {
        MonitorInfo {
            identity: MonitorIdentity::Unverified,
            index,
            scale,
            physical_position,
            physical_size: UVec2::new(2_560, 1_440),
        }
    }

    fn persisted() -> PersistedWindowState {
        PersistedWindowState {
            logical_position:  Some((-800, 100)),
            logical_width:     800,
            logical_height:    600,
            scale:             1.0,
            monitor:           1,
            saved_window_mode: SavedWindowMode::Windowed,
            app_name:          "test".to_string(),
        }
    }

    fn captured(monitor_snapshot: MonitorInfo, logical_offset: IVec2) -> CapturedWindowPlacement {
        CapturedWindowPlacement {
            monitor_snapshot,
            position: CapturedWindowPosition::Restorable { logical_offset },
            logical_size: UVec2::new(800, 600),
            saved_window_mode: SavedWindowMode::Windowed,
            captured_scale: monitor_snapshot.scale,
        }
    }

    #[test]
    fn startup_seed_is_read_once_and_promotes_only_explicitly() {
        let mut states = CapturedWindowStates::default();
        states.seed(HashMap::from([(WindowKey::Primary, persisted())]));
        states.seed(HashMap::new());

        assert!(states.startup_was_read());
        assert!(states.persisted(&WindowKey::Primary).is_some());

        let entity = Entity::PLACEHOLDER;
        states.bind(&WindowKey::Primary, entity);
        states.freeze(&WindowKey::Primary);
        assert_eq!(
            states.capture(
                WindowKey::Primary,
                entity,
                captured(monitor(1, 1.0, IVec2::new(-1_920, 0)), IVec2::new(120, 80)),
            ),
            StateMutation::Unchanged
        );
        assert!(states.persisted(&WindowKey::Primary).is_some());

        states.promote(
            WindowKey::Primary,
            entity,
            captured(monitor(1, 1.0, IVec2::new(-1_920, 0)), IVec2::new(120, 80)),
        );
        assert!(states.persisted(&WindowKey::Primary).is_none());
        assert_eq!(
            states
                .entry(&WindowKey::Primary)
                .map(|entry| entry.persistence),
            Some(PersistenceWriteState::Writable)
        );
    }

    #[test]
    fn persistence_policy_retains_remembered_and_frozen_absent_entries() {
        let remembered_key = WindowKey::Managed("remembered".to_string());
        let frozen_key = WindowKey::Managed("frozen".to_string());
        let mut states = CapturedWindowStates::default();
        states.seed(HashMap::from([
            (remembered_key.clone(), persisted()),
            (frozen_key.clone(), persisted()),
        ]));
        states.freeze(&frozen_key);

        assert_eq!(
            states.apply_policy(&ManagedWindowPersistence::RememberAll),
            StateMutation::Unchanged
        );
        assert!(states.entry(&remembered_key).is_some());

        assert_eq!(
            states.apply_policy(&ManagedWindowPersistence::ActiveOnly),
            StateMutation::Changed
        );
        assert!(states.entry(&remembered_key).is_none());
        assert!(states.entry(&frozen_key).is_some());
    }

    #[test]
    fn frozen_entry_survives_capture_unbind_policy_and_projection() {
        let key = WindowKey::Managed("recovering".to_string());
        let entity = Entity::PLACEHOLDER;
        let original = persisted();
        let mut states = CapturedWindowStates::default();
        states.seed(HashMap::from([(key.clone(), original.clone())]));
        states.bind(&key, entity);
        states.freeze(&key);

        states.capture(
            key.clone(),
            entity,
            captured(monitor(2, 2.0, IVec2::new(0, 0)), IVec2::new(20, 20)),
        );
        states.unbind(&key, entity);
        states.apply_policy(&ManagedWindowPersistence::ActiveOnly);

        assert_eq!(states.project("changed-app").get(&key), Some(&original));
    }

    #[test]
    fn live_cancellation_makes_the_captured_entry_writable() {
        let key = WindowKey::Managed("cancelled".to_string());
        let entity = Entity::PLACEHOLDER;
        let original = captured(monitor(0, 1.0, IVec2::ZERO), CANCELLED_ORIGINAL_OFFSET);
        let replacement = captured(monitor(1, 2.0, IVec2::ZERO), CANCELLED_REPLACEMENT_OFFSET);
        let mut states = CapturedWindowStates::default();
        states.capture(key.clone(), entity, original);
        states.freeze(&key);

        assert_eq!(
            states.cancel(&key, Some(entity), &ManagedWindowPersistence::RememberAll,),
            StateMutation::Unchanged
        );
        assert_eq!(
            states.capture(key.clone(), entity, replacement.clone()),
            StateMutation::Changed
        );
        assert_eq!(states.captured_placement(&key), Some(&replacement));
    }

    #[test]
    fn live_cancellation_promotes_a_persisted_only_entry_on_the_next_capture() {
        let key = WindowKey::Managed("cancelled-startup".to_string());
        let entity = Entity::PLACEHOLDER;
        let replacement = captured(monitor(1, 2.0, IVec2::ZERO), CANCELLED_REPLACEMENT_OFFSET);
        let mut states = CapturedWindowStates::default();
        states.seed(HashMap::from([(key.clone(), persisted())]));

        assert!(states.bind_and_freeze(&key, entity));
        assert_eq!(
            states.capture(key.clone(), entity, replacement.clone()),
            StateMutation::Unchanged
        );
        assert!(matches!(
            states.placement(&key),
            Some(CapturedPlacement::PersistedOnly(_))
        ));

        assert_eq!(
            states.cancel(&key, Some(entity), &ManagedWindowPersistence::RememberAll),
            StateMutation::Unchanged
        );
        assert_eq!(
            states.capture(key.clone(), entity, replacement.clone()),
            StateMutation::Changed
        );
        assert_eq!(states.captured_placement(&key), Some(&replacement));
        assert_eq!(
            states.entry(&key).map(|entry| entry.persistence),
            Some(PersistenceWriteState::Writable)
        );
    }

    #[test]
    fn remember_all_retained_write_preserves_the_application_name() {
        let key = WindowKey::Managed("remembered".to_string());
        let entity = Entity::PLACEHOLDER;
        let mut states = CapturedWindowStates::default();
        states.capture(
            key.clone(),
            entity,
            captured(monitor(0, 1.0, IVec2::ZERO), CANCELLED_ORIGINAL_OFFSET),
        );

        assert_eq!(
            states.deactivate(&key, entity, &ManagedWindowPersistence::RememberAll),
            StateMutation::Changed
        );
        let app_name = save::application_name();
        assert!(!app_name.is_empty());
        let projected = states.project("replacement-name");
        assert_eq!(
            projected.get(&key).map(|state| state.app_name.as_str()),
            Some(app_name.as_str())
        );

        let file = NamedTempFile::new();
        assert!(file.is_ok(), "temporary state file should be available");
        let Ok(file) = file else {
            return;
        };
        assert_eq!(
            save::save_all_states(file.path(), &projected),
            StateFileWrite::Written
        );
        let contents = fs::read_to_string(file.path());
        assert!(contents.is_ok(), "written state file should be readable");
        let Ok(contents) = contents else {
            return;
        };
        let decoded = format::decode(&contents);
        assert_eq!(
            decoded
                .as_ref()
                .and_then(|states| states.get(&key))
                .map(|state| state.app_name.as_str()),
            Some(app_name.as_str())
        );
    }

    #[test]
    fn relative_position_rebases_negative_origins_and_scale_changes() {
        let low_dpi = captured(
            monitor(1, 1.0, IVec2::new(-1_920, -200)),
            IVec2::new(160, 90),
        );
        assert_eq!(
            low_dpi.rebased_physical_position(&monitor(2, 2.0, IVec2::new(-3_840, -400))),
            Some(IVec2::new(-3_520, -220))
        );

        let high_dpi = captured(
            monitor(2, 2.0, IVec2::new(-3_840, -400)),
            IVec2::new(160, 90),
        );
        assert_eq!(
            high_dpi.rebased_physical_position(&monitor(1, 1.0, IVec2::new(-1_920, -200))),
            Some(IVec2::new(-1_760, -110))
        );
    }

    #[test]
    fn projection_adds_logical_offset_after_converting_fractional_scale_origin() {
        let placement = captured(monitor(1, 1.25, IVec2::new(2, -2)), IVec2::new(1, -1));

        assert_eq!(placement.project("test").logical_position, Some((3, -3)));
    }

    #[test]
    fn compositor_controlled_projection_has_no_position() {
        let placement = CapturedWindowPlacement {
            monitor_snapshot:  monitor(0, 2.0, IVec2::ZERO),
            position:          CapturedWindowPosition::CompositorControlled,
            logical_size:      UVec2::new(640, 480),
            saved_window_mode: SavedWindowMode::BorderlessFullscreen,
            captured_scale:    2.0,
        };

        assert_eq!(
            placement.rebased_physical_position(&placement.monitor_snapshot),
            None
        );
        assert_eq!(placement.project("wayland").logical_position, None);
    }

    #[test]
    fn capture_keeps_installed_snapshot_and_rebases_with_returned_monitor_metadata() {
        let installed = monitor(0, 1.0, IVec2::new(-1_920, 0));
        let same_entity_native_edit = monitor(0, 2.0, IVec2::new(-3_840, -200));
        let current_monitor = CurrentMonitor {
            monitor_info:          installed,
            effective_window_mode: WindowMode::Windowed,
        };
        let mut window = Window::default();
        window.resolution.set(800.0, 600.0);
        let placement = CapturedWindowPlacement::capture(
            &window,
            &current_monitor,
            Some(IVec2::new(-1_760, 90)),
            Platform::X11,
        );

        assert_eq!(placement.monitor_snapshot, installed);
        assert_ne!(placement.monitor_snapshot, same_entity_native_edit);

        let returned = monitor(0, 2.0, IVec2::new(-3_840, -200));
        assert_eq!(
            placement.rebased_physical_position(&returned),
            Some(IVec2::new(-3_520, -20))
        );
    }

    #[test]
    fn remember_all_reopens_captured_placement_without_overwriting_it() {
        let key = WindowKey::Managed("remembered".to_string());
        let first_entity = Entity::from_bits(1);
        let reopened_entity = Entity::from_bits(2);
        let retained = captured(monitor(1, 1.25, IVec2::new(2, -2)), IVec2::new(120, 80));
        let mut states = CapturedWindowStates::default();
        states.capture(key.clone(), first_entity, retained.clone());
        states.mark_clean();
        states.unbind(&key, first_entity);

        assert_eq!(
            states.apply_policy(&ManagedWindowPersistence::RememberAll),
            StateMutation::Unchanged
        );
        let restore_state = states.restore_state(&key);
        assert!(restore_state.is_some());
        assert_eq!(
            restore_state.and_then(|state| state.logical_position),
            retained.project("").logical_position
        );

        states.bind(&key, reopened_entity);
        states.freeze(&key);
        assert_eq!(
            states.capture(
                key.clone(),
                reopened_entity,
                captured(monitor(0, 1.0, IVec2::ZERO), IVec2::ZERO),
            ),
            StateMutation::Unchanged
        );
        assert!(!states.is_dirty());
        assert_eq!(
            states
                .restore_state(&key)
                .and_then(|state| state.logical_position),
            retained.project("").logical_position
        );
    }

    #[test]
    fn primary_marker_removal_unbinds_retained_state() {
        let mut app = App::new();
        app.insert_resource(ManagedWindowPersistence::RememberAll)
            .init_resource::<CapturedWindowStates>()
            .add_observer(on_primary_window_removed);
        let entity = app
            .world_mut()
            .spawn((Window::default(), PrimaryWindow))
            .id();
        app.world_mut()
            .resource_mut::<CapturedWindowStates>()
            .capture(
                WindowKey::Primary,
                entity,
                captured(monitor(0, 1.0, IVec2::ZERO), IVec2::new(30, 40)),
            );

        app.world_mut().entity_mut(entity).remove::<PrimaryWindow>();
        app.world_mut().flush();

        let entry = app
            .world()
            .resource::<CapturedWindowStates>()
            .entry(&WindowKey::Primary);
        assert!(entry.is_some());
        assert_eq!(entry.and_then(|entry| entry.live), None);
    }

    #[test]
    fn window_removal_applies_remember_all_then_active_only() {
        let remembered_key = WindowKey::Managed("remembered".to_string());
        let active_key = WindowKey::Managed("active".to_string());
        let mut app = App::new();
        app.insert_resource(ManagedWindowPersistence::RememberAll)
            .init_resource::<CapturedWindowStates>()
            .add_observer(on_window_removed);
        let remembered_entity = app.world_mut().spawn(Window::default()).id();
        app.world_mut()
            .resource_mut::<CapturedWindowStates>()
            .capture(
                remembered_key.clone(),
                remembered_entity,
                captured(monitor(0, 1.0, IVec2::ZERO), IVec2::new(30, 40)),
            );

        app.world_mut()
            .entity_mut(remembered_entity)
            .remove::<Window>();
        app.world_mut().flush();

        let remembered = app
            .world()
            .resource::<CapturedWindowStates>()
            .entry(&remembered_key);
        assert!(remembered.is_some());
        assert_eq!(remembered.and_then(|entry| entry.live), None);

        *app.world_mut().resource_mut::<ManagedWindowPersistence>() =
            ManagedWindowPersistence::ActiveOnly;
        let active_entity = app.world_mut().spawn(Window::default()).id();
        app.world_mut()
            .resource_mut::<CapturedWindowStates>()
            .capture(
                active_key.clone(),
                active_entity,
                captured(monitor(0, 1.0, IVec2::ZERO), IVec2::new(50, 60)),
            );

        app.world_mut().entity_mut(active_entity).remove::<Window>();
        app.world_mut().flush();

        let states = app.world().resource::<CapturedWindowStates>();
        assert!(states.entry(&remembered_key).is_none());
        assert!(states.entry(&active_key).is_none());
    }
}
