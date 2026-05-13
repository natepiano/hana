use bevy::prelude::*;

use super::OrbitCamBindings;
#[cfg(feature = "reflect-input-modes")]
use super::OrbitCamBindingsDescriptor;
#[cfg(feature = "reflect-input-modes")]
use super::OrbitCamBindingsError;
use super::OrbitCamInput;
use super::OrbitCamPreset;
use crate::orbit_cam::OrbitCam;
use crate::system_sets::OrbitCamInputInternalSet;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ActiveInputMode {
    Preset,
    Bindings,
    Manual,
}

#[derive(Clone, Copy, Debug, EntityEvent)]
pub(crate) struct OrbitCamInputModeReplaced {
    #[event_target]
    pub(crate) camera: Entity,
}

struct ModeReconciliation {
    camera:                Entity,
    selected:              ActiveInputMode,
    insert_default_preset: bool,
    replace_installation:  bool,
}

/// Manual input mode for an [`OrbitCam`].
///
/// This means the app writes [`OrbitCamInput`] through [`OrbitCamManualInput`].
/// It does not choose which camera receives ordinary routed input; later phases route
/// device input through `CameraInputRouting`.
///
/// [`OrbitCamManualInput`]: super::OrbitCamManualInput
#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq, Reflect)]
#[reflect(Component, Default)]
pub struct OrbitCamManual;

/// Mutable reflected draft for applying an orbit-camera input mode.
#[cfg(feature = "reflect-input-modes")]
#[derive(Component, Clone, Debug, Default, PartialEq, Reflect)]
#[reflect(Component, Default)]
pub struct OrbitCamInputModeDescriptor {
    /// Draft mode to validate and apply.
    pub mode: OrbitCamInputMode,
}

/// Reflected draft input-mode value.
#[cfg(feature = "reflect-input-modes")]
#[derive(Clone, Debug, PartialEq, Reflect)]
#[non_exhaustive]
pub enum OrbitCamInputMode {
    /// Built-in preset mode.
    Preset(OrbitCamPreset),
    /// Custom validated bindings mode.
    Bindings(OrbitCamBindingsDescriptor),
    /// Manual mode where app code writes camera intent.
    Manual,
}

#[cfg(feature = "reflect-input-modes")]
impl Default for OrbitCamInputMode {
    fn default() -> Self { Self::Preset(OrbitCamPreset::default()) }
}

/// Triggered when a reflected input-mode descriptor applies successfully.
#[cfg(feature = "reflect-input-modes")]
#[derive(Clone, Copy, Debug, EntityEvent, Reflect)]
#[reflect(Event, FromReflect)]
pub struct OrbitCamInputModeApplied {
    /// Camera entity whose descriptor applied.
    #[event_target]
    pub camera: Entity,
}

/// Triggered when a reflected input-mode descriptor is rejected.
#[cfg(feature = "reflect-input-modes")]
#[derive(Clone, Debug, EntityEvent)]
pub struct OrbitCamInputModeRejected {
    /// Camera entity whose descriptor was rejected.
    #[event_target]
    pub camera: Entity,
    /// Validation error that blocked descriptor application.
    pub error:  OrbitCamBindingsError,
}

/// Persisted feedback for the last descriptor apply attempt on a camera.
#[cfg(feature = "reflect-input-modes")]
#[derive(Component, Clone, Debug, Default, PartialEq, Eq, Reflect)]
#[reflect(Component, Default)]
pub struct OrbitCamInputModeApplyStatus {
    /// Last apply state.
    pub state:              OrbitCamInputModeApplyState,
    /// Error message from the last rejected apply attempt.
    pub last_error:         Option<String>,
    /// Frame index when an apply succeeded, when available.
    pub last_applied_frame: Option<u64>,
}

/// Point-in-time result of the last descriptor apply attempt.
#[cfg(feature = "reflect-input-modes")]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Reflect)]
#[non_exhaustive]
pub enum OrbitCamInputModeApplyState {
    /// The last apply attempt succeeded.
    #[default]
    Applied,
    /// The last apply attempt was rejected.
    Rejected,
}

#[derive(Component, Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct OrbitCamInputInstallation {
    entities: Vec<Entity>,
}

#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct OrbitCamInputInstallationOf(pub(crate) Entity);

#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct OrbitCamInputInstallationPlaceholder;

pub(crate) struct OrbitCamInputModesPlugin;

impl Plugin for OrbitCamInputModesPlugin {
    fn build(&self, app: &mut App) {
        app.add_observer(on_preset_mode_added)
            .add_observer(on_bindings_mode_added)
            .add_observer(on_manual_mode_added);

        #[cfg(feature = "reflect-input-modes")]
        app.add_systems(
            PreUpdate,
            (apply_input_mode_descriptors, reconcile_input_modes)
                .chain()
                .in_set(OrbitCamInputInternalSet::InputModes),
        );

        #[cfg(not(feature = "reflect-input-modes"))]
        app.add_systems(
            PreUpdate,
            reconcile_input_modes.in_set(OrbitCamInputInternalSet::InputModes),
        );
    }
}

fn on_preset_mode_added(
    added: On<Add, OrbitCamPreset>,
    mut commands: Commands,
    modes: Query<(Option<Ref<OrbitCamBindings>>, Option<Ref<OrbitCamManual>>)>,
) {
    let Ok((bindings, manual)) = modes.get(added.entity) else {
        return;
    };
    if bindings.is_some_and(|bindings| bindings.is_added())
        || manual.is_some_and(|manual| manual.is_added())
    {
        return;
    }

    commands
        .entity(added.entity)
        .remove::<OrbitCamBindings>()
        .remove::<OrbitCamManual>();
}

fn on_bindings_mode_added(
    added: On<Add, OrbitCamBindings>,
    mut commands: Commands,
    modes: Query<(Option<Ref<OrbitCamPreset>>, Option<Ref<OrbitCamManual>>)>,
) {
    let Ok((preset, manual)) = modes.get(added.entity) else {
        return;
    };
    if preset.is_some_and(|preset| preset.is_added())
        || manual.is_some_and(|manual| manual.is_added())
    {
        return;
    }

    commands
        .entity(added.entity)
        .remove::<OrbitCamPreset>()
        .remove::<OrbitCamManual>();
}

fn on_manual_mode_added(
    added: On<Add, OrbitCamManual>,
    mut commands: Commands,
    modes: Query<(Option<Ref<OrbitCamPreset>>, Option<Ref<OrbitCamBindings>>)>,
) {
    let Ok((preset, bindings)) = modes.get(added.entity) else {
        return;
    };
    if preset.is_some_and(|preset| preset.is_added())
        || bindings.is_some_and(|bindings| bindings.is_added())
    {
        return;
    }

    commands
        .entity(added.entity)
        .remove::<OrbitCamPreset>()
        .remove::<OrbitCamBindings>();
}

#[cfg(feature = "reflect-input-modes")]
fn apply_input_mode_descriptors(world: &mut World) {
    let mut query = world.query_filtered::<
        (Entity, &OrbitCamInputModeDescriptor),
        (With<OrbitCam>, Changed<OrbitCamInputModeDescriptor>),
    >();
    let changed_descriptors = query
        .iter(world)
        .map(|(camera, descriptor)| (camera, descriptor.mode.clone()))
        .collect::<Vec<_>>();

    for (camera, mode) in changed_descriptors {
        match apply_input_mode_descriptor(world, camera, mode) {
            Ok(()) => {
                world
                    .entity_mut(camera)
                    .insert(OrbitCamInputModeApplyStatus {
                        state:              OrbitCamInputModeApplyState::Applied,
                        last_error:         None,
                        last_applied_frame: None,
                    });
                world
                    .entity_mut(camera)
                    .trigger(|camera| OrbitCamInputModeApplied { camera });
            },
            Err(error) => {
                world
                    .entity_mut(camera)
                    .insert(OrbitCamInputModeApplyStatus {
                        state:              OrbitCamInputModeApplyState::Rejected,
                        last_error:         Some(error.to_string()),
                        last_applied_frame: None,
                    });
                warn!("rejected OrbitCam input-mode descriptor for {camera}: {error}");
                world
                    .entity_mut(camera)
                    .trigger(|camera| OrbitCamInputModeRejected { camera, error });
            },
        }
    }
}

#[cfg(feature = "reflect-input-modes")]
fn apply_input_mode_descriptor(
    world: &mut World,
    camera: Entity,
    mode: OrbitCamInputMode,
) -> Result<(), OrbitCamBindingsError> {
    let mut entity = world.entity_mut(camera);
    match mode {
        OrbitCamInputMode::Preset(preset) => {
            entity
                .insert(preset)
                .remove::<OrbitCamBindings>()
                .remove::<OrbitCamManual>();
            Ok(())
        },
        OrbitCamInputMode::Bindings(descriptor) => {
            let bindings = OrbitCamBindings::try_from(descriptor)?;
            entity
                .insert(bindings)
                .remove::<OrbitCamPreset>()
                .remove::<OrbitCamManual>();
            Ok(())
        },
        OrbitCamInputMode::Manual => {
            entity
                .insert(OrbitCamManual)
                .remove::<OrbitCamPreset>()
                .remove::<OrbitCamBindings>();
            Ok(())
        },
    }
}

fn reconcile_input_modes(world: &mut World) {
    let mut query = world.query_filtered::<(
        Entity,
        Option<Ref<OrbitCamPreset>>,
        Option<Ref<OrbitCamBindings>>,
        Option<Ref<OrbitCamManual>>,
        Option<&OrbitCamInputInstallation>,
    ), With<OrbitCam>>();
    let reconciliations = query
        .iter(world)
        .map(|(camera, preset, bindings, manual, installation)| {
            let selected =
                select_input_mode(preset.as_deref(), bindings.as_deref(), manual.as_deref());
            let mode_count = usize::from(preset.is_some())
                + usize::from(bindings.is_some())
                + usize::from(manual.is_some());
            let selected_changed = match selected {
                ActiveInputMode::Preset => preset
                    .as_ref()
                    .is_some_and(|preset| preset.is_added() || preset.is_changed()),
                ActiveInputMode::Bindings => bindings
                    .as_ref()
                    .is_some_and(|bindings| bindings.is_added() || bindings.is_changed()),
                ActiveInputMode::Manual => manual
                    .as_ref()
                    .is_some_and(|manual| manual.is_added() || manual.is_changed()),
            };

            ModeReconciliation {
                camera,
                selected,
                insert_default_preset: mode_count == 0,
                replace_installation: mode_count != 1 || selected_changed || installation.is_none(),
            }
        })
        .collect::<Vec<_>>();

    for reconciliation in reconciliations {
        apply_mode_reconciliation(world, reconciliation);
    }
}

const fn select_input_mode(
    _preset: Option<&OrbitCamPreset>,
    bindings: Option<&OrbitCamBindings>,
    manual: Option<&OrbitCamManual>,
) -> ActiveInputMode {
    if manual.is_some() {
        ActiveInputMode::Manual
    } else if bindings.is_some() {
        ActiveInputMode::Bindings
    } else {
        ActiveInputMode::Preset
    }
}

fn apply_mode_reconciliation(world: &mut World, reconciliation: ModeReconciliation) {
    {
        let mut entity = world.entity_mut(reconciliation.camera);
        match reconciliation.selected {
            ActiveInputMode::Preset => {
                if reconciliation.insert_default_preset {
                    entity.insert(OrbitCamPreset::default());
                    warn!(
                        "restored default OrbitCam input mode for {:?}",
                        reconciliation.camera
                    );
                }
                entity
                    .remove::<OrbitCamBindings>()
                    .remove::<OrbitCamManual>();
            },
            ActiveInputMode::Bindings => {
                entity.remove::<OrbitCamPreset>().remove::<OrbitCamManual>();
            },
            ActiveInputMode::Manual => {
                entity
                    .remove::<OrbitCamPreset>()
                    .remove::<OrbitCamBindings>();
            },
        }
    }

    if reconciliation.replace_installation {
        clear_orbit_cam_input(world, reconciliation.camera);
        replace_input_installation(world, reconciliation.camera, reconciliation.selected);
        world
            .entity_mut(reconciliation.camera)
            .trigger(|camera| OrbitCamInputModeReplaced { camera });
    }
}

fn clear_orbit_cam_input(world: &mut World, camera: Entity) {
    if let Some(mut input) = world.get_mut::<OrbitCamInput>(camera) {
        input.clear();
    }
}

fn replace_input_installation(world: &mut World, camera: Entity, mode: ActiveInputMode) {
    for installed_entity in installed_input_entities(world, camera) {
        let _ = world.despawn(installed_entity);
    }

    let entities = match mode {
        ActiveInputMode::Preset | ActiveInputMode::Bindings => {
            vec![
                world
                    .spawn((
                        OrbitCamInputInstallationOf(camera),
                        OrbitCamInputInstallationPlaceholder,
                    ))
                    .id(),
            ]
        },
        ActiveInputMode::Manual => Vec::new(),
    };

    world
        .entity_mut(camera)
        .insert(OrbitCamInputInstallation { entities });
}

pub(crate) fn installed_input_entities(world: &World, camera: Entity) -> Vec<Entity> {
    let Some(installation) = world.get::<OrbitCamInputInstallation>(camera) else {
        return Vec::new();
    };

    installation
        .entities
        .iter()
        .copied()
        .filter(|entity| {
            world
                .get::<OrbitCamInputInstallationOf>(*entity)
                .is_some_and(|owner| owner.0 == camera)
        })
        .collect()
}

pub(crate) fn input_installation_has_placeholder(world: &World, camera: Entity) -> bool {
    installed_input_entities(world, camera)
        .iter()
        .any(|entity| {
            world
                .get::<OrbitCamInputInstallationPlaceholder>(*entity)
                .is_some()
        })
}

pub(crate) fn replace_installed_input_entities(
    world: &mut World,
    camera: Entity,
    entities: Vec<Entity>,
) {
    world
        .entity_mut(camera)
        .insert(OrbitCamInputInstallation { entities });
}

#[cfg(test)]
mod tests {
    use bevy::prelude::*;

    use super::*;
    use crate::input::OrbitCamInputModeDescriptor;
    use crate::input::OrbitCamInputModeRejected;
    use crate::input::OrbitCamManualInputWriter;
    use crate::input::bindings;
    use crate::system_sets::LagrangeSystemSetsPlugin;

    #[derive(Resource)]
    struct ManualWriterTestCamera {
        manual: Entity,
        preset: Entity,
    }

    #[derive(Resource, Default)]
    struct ManualWriterTestResult {
        manual_written:  bool,
        preset_rejected: bool,
    }

    #[derive(Resource, Default)]
    struct ModeReplacementEvents(usize);

    fn test_app() -> App {
        let mut app = App::new();
        app.add_plugins((
            MinimalPlugins,
            LagrangeSystemSetsPlugin,
            OrbitCamInputModesPlugin,
        ));
        app
    }

    #[test]
    fn preinput_restores_default_preset_when_mode_is_missing() {
        let mut app = test_app();
        let camera = app
            .world_mut()
            .spawn((OrbitCam::default(), OrbitCamInput::default()))
            .id();

        app.update();

        assert_eq!(
            app.world().get::<OrbitCamPreset>(camera),
            Some(&OrbitCamPreset::SimpleMouse)
        );
        assert_eq!(installed_input_entities(app.world(), camera).len(), 1);
    }

    #[test]
    fn preinput_enforces_manual_over_other_modes() {
        let mut app = test_app();
        let camera = app
            .world_mut()
            .spawn((
                OrbitCam::default(),
                OrbitCamInput::default(),
                OrbitCamPreset::BlenderLike,
                OrbitCamManual,
            ))
            .id();

        app.update();

        assert!(app.world().get::<OrbitCamManual>(camera).is_some());
        assert!(app.world().get::<OrbitCamPreset>(camera).is_none());
        assert!(app.world().get::<OrbitCamBindings>(camera).is_none());
        assert!(installed_input_entities(app.world(), camera).is_empty());
    }

    #[test]
    fn descriptor_applies_preset_mode() {
        let mut app = test_app();
        let camera = app
            .world_mut()
            .spawn((
                OrbitCam::default(),
                OrbitCamInput::default(),
                OrbitCamInputModeDescriptor {
                    mode: OrbitCamInputMode::Preset(OrbitCamPreset::BlenderLike),
                },
            ))
            .id();

        app.update();

        assert_eq!(
            app.world().get::<OrbitCamPreset>(camera),
            Some(&OrbitCamPreset::BlenderLike)
        );
        assert_eq!(
            app.world()
                .get::<OrbitCamInputModeApplyStatus>(camera)
                .map(|status| status.state),
            Some(OrbitCamInputModeApplyState::Applied)
        );
    }

    #[test]
    fn descriptor_rejection_keeps_previous_mode() {
        let mut app = test_app();
        let camera = app
            .world_mut()
            .spawn((
                OrbitCam::default(),
                OrbitCamInput::default(),
                OrbitCamPreset::BlenderLike,
                OrbitCamInputModeDescriptor {
                    mode: OrbitCamInputMode::Bindings(
                        bindings::invalid_bindings_descriptor_for_tests(),
                    ),
                },
            ))
            .id();

        app.world_mut()
            .entity_mut(camera)
            .observe(|_rejected: On<OrbitCamInputModeRejected>| {});
        app.update();

        assert_eq!(
            app.world().get::<OrbitCamPreset>(camera),
            Some(&OrbitCamPreset::BlenderLike)
        );
        assert_eq!(
            app.world()
                .get::<OrbitCamInputModeApplyStatus>(camera)
                .map(|status| status.state),
            Some(OrbitCamInputModeApplyState::Rejected)
        );
    }

    fn write_manual_test_input(
        mut writer: OrbitCamManualInputWriter,
        cameras: Res<ManualWriterTestCamera>,
        mut result: ResMut<ManualWriterTestResult>,
    ) {
        result.manual_written = writer
            .get_mut(cameras.manual, super::super::ManualInputSource::manual())
            .is_ok();
        result.preset_rejected = writer
            .get_mut(cameras.preset, super::super::ManualInputSource::manual())
            .is_err();
    }

    #[test]
    fn manual_writer_only_yields_manual_cameras() {
        let mut app = test_app();
        let manual = app
            .world_mut()
            .spawn((
                OrbitCam::default(),
                OrbitCamInput::default(),
                OrbitCamManual,
            ))
            .id();
        let preset = app
            .world_mut()
            .spawn((
                OrbitCam::default(),
                OrbitCamInput::default(),
                OrbitCamPreset::SimpleMouse,
            ))
            .id();
        app.insert_resource(ManualWriterTestCamera { manual, preset })
            .init_resource::<ManualWriterTestResult>()
            .add_systems(Update, write_manual_test_input);

        app.update();

        let result = app.world().resource::<ManualWriterTestResult>();
        assert!(result.manual_written);
        assert!(result.preset_rejected);
    }

    #[test]
    fn mode_replacement_triggers_internal_hook() {
        let mut app = test_app();
        let camera = app
            .world_mut()
            .spawn((
                OrbitCam::default(),
                OrbitCamInput::default(),
                OrbitCamManual,
            ))
            .id();
        app.init_resource::<ModeReplacementEvents>();
        app.world_mut().entity_mut(camera).observe(
            |_replaced: On<OrbitCamInputModeReplaced>,
             mut events: ResMut<ModeReplacementEvents>| {
                events.0 += 1;
            },
        );

        app.update();

        assert_eq!(app.world().resource::<ModeReplacementEvents>().0, 1);
    }
}
