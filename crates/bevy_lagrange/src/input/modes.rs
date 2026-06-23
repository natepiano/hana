use bevy::prelude::*;
use bevy_enhanced_input::prelude::Actions;

use super::OrbitCamBindings;
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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum InstallationStatus {
    Present,
    Missing,
}

impl InstallationStatus {
    const fn from_installation(installation: Option<&OrbitCamInputInstallation>) -> Self {
        match installation {
            Some(_) => Self::Present,
            None => Self::Missing,
        }
    }

    const fn is_present(self) -> bool { matches!(self, Self::Present) }
}

#[derive(Clone, Debug, Default, PartialEq)]
struct PreparedRuntimeInputMode {
    bindings: Option<OrbitCamBindings>,
}

impl PreparedRuntimeInputMode {
    const fn bindings(bindings: OrbitCamBindings) -> Self {
        Self {
            bindings: Some(bindings),
        }
    }

    const fn mode(&self) -> RuntimeInputMode {
        if self.bindings.is_some() {
            RuntimeInputMode::Bindings
        } else {
            RuntimeInputMode::Manual
        }
    }
}

#[derive(Clone, Copy, Debug, EntityEvent)]
pub(crate) struct OrbitCamInputModeReplaced {
    #[event_target]
    pub(crate) camera: Entity,
}

#[derive(Clone, Debug, PartialEq)]
struct ModeReconciliation {
    camera:       Entity,
    mode:         OrbitCamInputMode,
    last_valid:   Option<OrbitCamInputMode>,
    installation: InstallationStatus,
}

/// Selected input mode for an [`OrbitCam`].
///
/// `OrbitCam` requires this component and defaults to simple mouse preset input.
/// Use `Preset` for a built-in keymap, `Bindings` for app-owned validated
/// bindings, or `Manual` when app code writes camera intent through
/// [`OrbitCamManualInputWriter`].
///
/// [`OrbitCamManualInputWriter`]: super::OrbitCamManualInputWriter
#[derive(Component, Clone, Debug, PartialEq, Reflect)]
#[reflect(Component, Default)]
#[non_exhaustive]
#[allow(
    clippy::large_enum_variant,
    reason = "one OrbitCamInputMode Component exists per camera (a handful ever), so \
              inlining the full binding set in `Bindings` is cheaper than a per-camera \
              heap indirection from boxing"
)]
pub enum OrbitCamInputMode {
    /// Built-in preset mode.
    Preset(OrbitCamPreset),
    /// Custom validated bindings mode.
    Bindings(OrbitCamBindings),
    /// Manual mode where app code writes camera intent.
    Manual,
}

impl Default for OrbitCamInputMode {
    fn default() -> Self { Self::with_preset(OrbitCamPreset::default()) }
}

impl OrbitCamInputMode {
    /// Builds preset input mode from a built-in orbit-camera preset.
    #[must_use]
    pub fn with_preset(preset: impl Into<OrbitCamPreset>) -> Self { Self::Preset(preset.into()) }
}

impl From<OrbitCamPreset> for OrbitCamInputMode {
    fn from(preset: OrbitCamPreset) -> Self { Self::with_preset(preset) }
}

impl From<OrbitCamBindings> for OrbitCamInputMode {
    fn from(bindings: OrbitCamBindings) -> Self { Self::Bindings(bindings) }
}

/// Runtime marker for cameras using manual app-authored input.
#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct OrbitCamManual;

#[derive(Component, Clone, Debug, PartialEq)]
pub(crate) struct OrbitCamResolvedBindings(pub(crate) OrbitCamBindings);

#[derive(Component, Clone, Debug, PartialEq)]
struct OrbitCamLastValidInputMode(OrbitCamInputMode);

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
        app.add_systems(
            PreUpdate,
            reconcile_input_modes.in_set(OrbitCamInputInternalSet::InputModes),
        );
    }
}

fn reconcile_input_modes(world: &mut World) {
    let mut query = world.query_filtered::<(
        Entity,
        &OrbitCamInputMode,
        Option<&OrbitCamLastValidInputMode>,
        Option<&OrbitCamInputInstallation>,
    ), (
        With<OrbitCam>,
        Or<(
            Changed<OrbitCamInputMode>,
            Without<OrbitCamInputInstallation>,
        )>,
    )>();
    let reconciliations = query
        .iter(world)
        .map(
            |(camera, mode, last_valid, installation)| ModeReconciliation {
                camera,
                mode: mode.clone(),
                last_valid: last_valid.map(|mode| mode.0.clone()),
                installation: InstallationStatus::from_installation(installation),
            },
        )
        .collect::<Vec<_>>();

    for reconciliation in reconciliations {
        apply_mode_reconciliation(world, reconciliation);
    }
}

fn apply_mode_reconciliation(world: &mut World, reconciliation: ModeReconciliation) {
    if reconciliation.installation.is_present()
        && reconciliation
            .last_valid
            .as_ref()
            .is_some_and(|last_valid| *last_valid == reconciliation.mode)
    {
        return;
    }
    let prepared_mode = match prepare_runtime_input_mode(&reconciliation.mode) {
        Ok(prepared_mode) => prepared_mode,
        Err(error) => {
            restore_last_valid_input_mode(world, &reconciliation, error);
            return;
        },
    };

    clear_orbit_cam_input(world, reconciliation.camera);
    let runtime_mode = prepared_mode.mode();
    apply_prepared_runtime_input_mode(world, reconciliation.camera, prepared_mode);
    replace_input_installation(world, reconciliation.camera, runtime_mode);
    world
        .entity_mut(reconciliation.camera)
        .insert(OrbitCamLastValidInputMode(reconciliation.mode));
    world
        .entity_mut(reconciliation.camera)
        .trigger(|camera| OrbitCamInputModeReplaced { camera });
}

fn prepare_runtime_input_mode(
    mode: &OrbitCamInputMode,
) -> Result<PreparedRuntimeInputMode, OrbitCamBindingsError> {
    match mode {
        OrbitCamInputMode::Preset(preset) => {
            Ok(PreparedRuntimeInputMode::bindings(preset.to_bindings()?))
        },
        OrbitCamInputMode::Bindings(bindings) => {
            Ok(PreparedRuntimeInputMode::bindings(bindings.clone()))
        },
        OrbitCamInputMode::Manual => Ok(PreparedRuntimeInputMode::default()),
    }
}

fn apply_prepared_runtime_input_mode(
    world: &mut World,
    camera: Entity,
    prepared_mode: PreparedRuntimeInputMode,
) {
    match prepared_mode.bindings {
        Some(bindings) => {
            world
                .entity_mut(camera)
                .insert(OrbitCamResolvedBindings(bindings))
                .remove::<OrbitCamManual>();
        },
        None => {
            world
                .entity_mut(camera)
                .remove::<OrbitCamResolvedBindings>()
                .insert(OrbitCamManual);
        },
    }
}

fn restore_last_valid_input_mode(
    world: &mut World,
    reconciliation: &ModeReconciliation,
    error: OrbitCamBindingsError,
) {
    warn!(
        "failed to build OrbitCam input mode for {:?}: {error}",
        reconciliation.camera
    );
    if let Some(last_valid) = &reconciliation.last_valid {
        world
            .entity_mut(reconciliation.camera)
            .insert(last_valid.clone());
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
    use crate::input::CameraInteractionSources;
    use crate::input::OrbitCamBlenderLikePreset;
    use crate::input::OrbitCamManualInputWriter;
    use crate::input::OrbitCamSensitivity;
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

    type TestResult = Result<(), &'static str>;

    const INVALID_SOURCE_SENSITIVITY: f32 = -1.0;

    fn test_app() -> App {
        let mut app = App::new();
        app.add_plugins((
            MinimalPlugins,
            LagrangeSystemSetsPlugin,
            OrbitCamInputModesPlugin,
        ));
        app
    }

    fn invalid_blender_like_preset() -> OrbitCamPreset {
        OrbitCamBlenderLikePreset::default()
            .mouse_sensitivity(OrbitCamSensitivity::uniform(INVALID_SOURCE_SENSITIVITY))
            .into()
    }

    fn mark_current_input(app: &mut App, camera: Entity) -> TestResult {
        app.world_mut()
            .get_mut::<OrbitCamInput>(camera)
            .ok_or("camera should have OrbitCamInput")?
            .orbit_pixels_with_sources(Vec2::new(1.0, 0.0), CameraInteractionSources::MOUSE);
        Ok(())
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
            Some(&OrbitCamInputMode::with_preset(
                OrbitCamPreset::simple_mouse()
            ))
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
    fn invalid_direct_preset_replacement_keeps_previous_runtime_state() -> TestResult {
        let mut app = test_app();
        let valid_mode = OrbitCamInputMode::with_preset(OrbitCamPreset::blender_like());
        let camera = app
            .world_mut()
            .spawn((
                OrbitCam::default(),
                OrbitCamInput::default(),
                valid_mode.clone(),
            ))
            .id();

        app.update();
        mark_current_input(&mut app, camera)?;
        let previous_bindings = app
            .world()
            .get::<OrbitCamResolvedBindings>(camera)
            .cloned()
            .ok_or("camera should have resolved bindings")?;
        let previous_installation = installed_input_entities(app.world(), camera);
        app.init_resource::<ModeReplacementEvents>();
        app.world_mut().entity_mut(camera).observe(
            |_replaced: On<OrbitCamInputModeReplaced>,
             mut events: ResMut<ModeReplacementEvents>| {
                events.0 += 1;
            },
        );

        app.world_mut()
            .entity_mut(camera)
            .insert(OrbitCamInputMode::with_preset(invalid_blender_like_preset()));
        app.update();

        assert_eq!(
            app.world().get::<OrbitCamInputMode>(camera),
            Some(&valid_mode)
        );
        assert_eq!(
            app.world().get::<OrbitCamResolvedBindings>(camera),
            Some(&previous_bindings)
        );
        assert_eq!(
            installed_input_entities(app.world(), camera),
            previous_installation
        );
        assert!(
            app.world()
                .get::<OrbitCamInput>(camera)
                .is_some_and(OrbitCamInput::has_orbit)
        );
        assert_eq!(app.world().resource::<ModeReplacementEvents>().0, 0);

        Ok(())
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
                OrbitCamInputMode::with_preset(OrbitCamPreset::simple_mouse()),
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
