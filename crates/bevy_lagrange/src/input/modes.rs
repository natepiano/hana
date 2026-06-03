use bevy::prelude::*;
use bevy_enhanced_input::prelude::Actions;

use super::OrbitCamBindings;
#[cfg(feature = "reflect-input-modes")]
use super::OrbitCamBindingsDescriptor;
#[cfg(feature = "reflect-input-modes")]
use super::OrbitCamBindingsError;
use super::OrbitCamInput;
use super::OrbitCamInputContext;
use super::OrbitCamPreset;
use crate::orbit_cam::OrbitCam;
use crate::system_sets::OrbitCamInputInternalSet;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum RuntimeInputMode {
    Bindings,
    Manual,
}

#[derive(Clone, Copy, Debug, EntityEvent)]
pub(crate) struct OrbitCamInputModeReplaced {
    #[event_target]
    pub(crate) camera: Entity,
}

#[derive(Clone, Debug, PartialEq)]
struct ModeReconciliation {
    camera: Entity,
    mode:   OrbitCamInputMode,
}

/// Selected input mode for an [`OrbitCam`].
///
/// `OrbitCam` requires this component and defaults to
/// `OrbitCamInputMode::Preset(OrbitCamPreset::SimpleMouse)`. Use `Preset` for a
/// built-in keymap, `Bindings` for app-owned validated bindings, or `Manual`
/// when app code writes camera intent through [`OrbitCamManualInputWriter`].
///
/// [`OrbitCamManualInputWriter`]: super::OrbitCamManualInputWriter
#[derive(Component, Clone, Debug, PartialEq, Reflect)]
#[reflect(Component, Default)]
#[non_exhaustive]
pub enum OrbitCamInputMode {
    /// Built-in preset mode.
    Preset(OrbitCamPreset),
    /// Custom validated bindings mode.
    Bindings(OrbitCamBindings),
    /// Manual mode where app code writes camera intent.
    Manual,
}

impl Default for OrbitCamInputMode {
    fn default() -> Self { Self::Preset(OrbitCamPreset::default()) }
}

impl From<OrbitCamPreset> for OrbitCamInputMode {
    fn from(preset: OrbitCamPreset) -> Self { Self::Preset(preset) }
}

impl From<OrbitCamBindings> for OrbitCamInputMode {
    fn from(bindings: OrbitCamBindings) -> Self { Self::Bindings(bindings) }
}

/// Runtime marker for cameras using manual app-authored input.
#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct OrbitCamManual;

#[derive(Component, Clone, Debug, PartialEq)]
pub(crate) struct OrbitCamResolvedBindings(pub(crate) OrbitCamBindings);

/// Mutable reflected draft for applying an orbit-camera input mode.
#[cfg(feature = "reflect-input-modes")]
#[derive(Component, Clone, Debug, Default, PartialEq, Reflect)]
#[reflect(Component, Default)]
pub struct OrbitCamInputModeDescriptor {
    /// Draft mode to validate and apply.
    pub mode: OrbitCamInputModeDraft,
}

/// Reflected draft input-mode value.
#[cfg(feature = "reflect-input-modes")]
#[derive(Clone, Debug, PartialEq, Reflect)]
#[non_exhaustive]
pub enum OrbitCamInputModeDraft {
    /// Built-in preset mode.
    Preset(OrbitCamPreset),
    /// Custom binding descriptor mode.
    Bindings(OrbitCamBindingsDescriptor),
    /// Manual mode where app code writes camera intent.
    Manual,
}

#[cfg(feature = "reflect-input-modes")]
impl Default for OrbitCamInputModeDraft {
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
    mode: OrbitCamInputModeDraft,
) -> Result<(), OrbitCamBindingsError> {
    let mut entity = world.entity_mut(camera);
    match mode {
        OrbitCamInputModeDraft::Preset(preset) => {
            entity.insert(OrbitCamInputMode::Preset(preset));
            Ok(())
        },
        OrbitCamInputModeDraft::Bindings(descriptor) => {
            let bindings = OrbitCamBindings::try_from(descriptor)?;
            entity.insert(OrbitCamInputMode::Bindings(bindings));
            Ok(())
        },
        OrbitCamInputModeDraft::Manual => {
            entity.insert(OrbitCamInputMode::Manual);
            Ok(())
        },
    }
}

fn reconcile_input_modes(world: &mut World) {
    let mut query = world.query_filtered::<(Entity, &OrbitCamInputMode), (
        With<OrbitCam>,
        Or<(
            Changed<OrbitCamInputMode>,
            Without<OrbitCamInputInstallation>,
        )>,
    )>();
    let reconciliations = query
        .iter(world)
        .map(|(camera, mode)| ModeReconciliation {
            camera,
            mode: mode.clone(),
        })
        .collect::<Vec<_>>();

    for reconciliation in reconciliations {
        apply_mode_reconciliation(world, reconciliation);
    }
}

fn apply_mode_reconciliation(world: &mut World, reconciliation: ModeReconciliation) {
    clear_orbit_cam_input(world, reconciliation.camera);
    let runtime_mode = apply_runtime_input_mode(world, &reconciliation);
    replace_input_installation(world, reconciliation.camera, runtime_mode);
    world
        .entity_mut(reconciliation.camera)
        .trigger(|camera| OrbitCamInputModeReplaced { camera });
}

fn apply_runtime_input_mode(
    world: &mut World,
    reconciliation: &ModeReconciliation,
) -> RuntimeInputMode {
    match &reconciliation.mode {
        OrbitCamInputMode::Preset(preset) => {
            match preset.to_bindings() {
                Ok(bindings) => {
                    world
                        .entity_mut(reconciliation.camera)
                        .insert(OrbitCamResolvedBindings(bindings))
                        .remove::<OrbitCamManual>();
                },
                Err(error) => {
                    warn!(
                        "failed to build OrbitCam preset bindings for {:?}: {error}",
                        reconciliation.camera
                    );
                    world
                        .entity_mut(reconciliation.camera)
                        .remove::<OrbitCamResolvedBindings>()
                        .insert(OrbitCamManual);
                    return RuntimeInputMode::Manual;
                },
            }
            RuntimeInputMode::Bindings
        },
        OrbitCamInputMode::Bindings(bindings) => {
            world
                .entity_mut(reconciliation.camera)
                .insert(OrbitCamResolvedBindings(bindings.clone()))
                .remove::<OrbitCamManual>();
            RuntimeInputMode::Bindings
        },
        OrbitCamInputMode::Manual => {
            world
                .entity_mut(reconciliation.camera)
                .remove::<OrbitCamResolvedBindings>()
                .insert(OrbitCamManual);
            RuntimeInputMode::Manual
        },
    }
}

fn clear_orbit_cam_input(world: &mut World, camera: Entity) {
    if let Some(mut input) = world.get_mut::<OrbitCamInput>(camera) {
        input.clear();
    }
}

fn replace_input_installation(world: &mut World, camera: Entity, mode: RuntimeInputMode) {
    // Drop the previous installation through the `Actions` relationship rather
    // than iterating `installed_input_entities`. That flat list holds both the
    // action entities and their `BindingOf` children; despawning an action
    // already despawns its bindings through `linked_spawn`, so iterating the list
    // despawned each binding a second time after its action was gone — which
    // `World::despawn` reports as despawning an absent entity. `despawn_related`
    // walks each action subtree exactly once and skips entities already despawned.
    world
        .entity_mut(camera)
        .despawn_related::<Actions<OrbitCamInputContext>>();

    let entities = match mode {
        RuntimeInputMode::Bindings => {
            vec![
                world
                    .spawn((
                        OrbitCamInputInstallationOf(camera),
                        OrbitCamInputInstallationPlaceholder,
                    ))
                    .id(),
            ]
        },
        RuntimeInputMode::Manual => Vec::new(),
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
    fn preinput_lowers_required_default_preset() {
        let mut app = test_app();
        let camera = app
            .world_mut()
            .spawn((OrbitCam::default(), OrbitCamInput::default()))
            .id();

        app.update();

        assert_eq!(
            app.world().get::<OrbitCamInputMode>(camera),
            Some(&OrbitCamInputMode::Preset(OrbitCamPreset::SimpleMouse))
        );
        assert!(
            app.world()
                .get::<OrbitCamResolvedBindings>(camera)
                .is_some()
        );
        assert_eq!(installed_input_entities(app.world(), camera).len(), 1);
    }

    #[test]
    fn preinput_lowers_manual_mode() {
        let mut app = test_app();
        let camera = app
            .world_mut()
            .spawn((
                OrbitCam::default(),
                OrbitCamInput::default(),
                OrbitCamInputMode::Manual,
            ))
            .id();

        app.update();

        assert!(app.world().get::<OrbitCamManual>(camera).is_some());
        assert!(
            app.world()
                .get::<OrbitCamResolvedBindings>(camera)
                .is_none()
        );
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
                    mode: OrbitCamInputModeDraft::Preset(OrbitCamPreset::BlenderLike),
                },
            ))
            .id();

        app.update();

        assert_eq!(
            app.world().get::<OrbitCamInputMode>(camera),
            Some(&OrbitCamInputMode::Preset(OrbitCamPreset::BlenderLike))
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
                OrbitCamInputMode::Preset(OrbitCamPreset::BlenderLike),
                OrbitCamInputModeDescriptor {
                    mode: OrbitCamInputModeDraft::Bindings(
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
            app.world().get::<OrbitCamInputMode>(camera),
            Some(&OrbitCamInputMode::Preset(OrbitCamPreset::BlenderLike))
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
                OrbitCamInputMode::Manual,
            ))
            .id();
        let preset = app
            .world_mut()
            .spawn((
                OrbitCam::default(),
                OrbitCamInput::default(),
                OrbitCamInputMode::Preset(OrbitCamPreset::SimpleMouse),
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
                OrbitCamInputMode::Manual,
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
