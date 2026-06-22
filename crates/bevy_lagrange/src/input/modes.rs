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
#[cfg(feature = "reflect-input-modes")]
use super::OrbitCamPresetDraft;
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
#[reflect(Default)]
#[non_exhaustive]
#[allow(
    clippy::large_enum_variant,
    reason = "draft mirror of OrbitCamInputMode; one per camera at most, so inlining \
              the binding descriptor in `Bindings` beats a per-camera heap indirection"
)]
pub enum OrbitCamInputModeDraft {
    /// Built-in preset mode.
    Preset(OrbitCamPresetDraft),
    /// Custom binding descriptor mode.
    Bindings(OrbitCamBindingsDescriptor),
    /// Manual mode where app code writes camera intent.
    Manual,
}

#[cfg(feature = "reflect-input-modes")]
impl Default for OrbitCamInputModeDraft {
    fn default() -> Self { Self::Preset(OrbitCamPresetDraft::default()) }
}

#[cfg(feature = "reflect-input-modes")]
impl OrbitCamInputModeDraft {
    /// Builds a reflected preset draft from an authored runtime preset payload.
    #[must_use]
    pub fn from_preset(preset: &OrbitCamPreset) -> Self {
        Self::Preset(OrbitCamPresetDraft::from_preset(preset))
    }

    /// Builds a reflected draft for manual app-authored input.
    #[must_use]
    pub const fn manual() -> Self { Self::Manual }
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
    let mode = validated_input_mode_from_draft(mode)?;
    let mut entity = world.entity_mut(camera);
    entity.insert(mode);
    Ok(())
}

#[cfg(feature = "reflect-input-modes")]
fn validated_input_mode_from_draft(
    mode: OrbitCamInputModeDraft,
) -> Result<OrbitCamInputMode, OrbitCamBindingsError> {
    match mode {
        OrbitCamInputModeDraft::Preset(preset) => {
            let preset = OrbitCamPreset::try_from(preset)?;
            let mode = OrbitCamInputMode::with_preset(preset);
            prepare_runtime_input_mode(&mode)?;
            Ok(mode)
        },
        OrbitCamInputModeDraft::Bindings(descriptor) => {
            let bindings = OrbitCamBindings::try_from(descriptor)?;
            Ok(OrbitCamInputMode::Bindings(bindings))
        },
        OrbitCamInputModeDraft::Manual => Ok(OrbitCamInputMode::Manual),
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
    use bevy::camera::RenderTarget;
    use bevy::input::gestures::PinchGesture;
    use bevy::input::mouse::AccumulatedMouseMotion;
    use bevy::input::mouse::AccumulatedMouseScroll;
    use bevy::prelude::*;
    use bevy::window::WindowRef;
    use bevy_enhanced_input::prelude::ModKeys;

    use super::*;
    use crate::enhanced_input::LagrangeEnhancedInputPlugin;
    use crate::input;
    use crate::input::CameraInputRoutingConfig;
    use crate::input::CameraInteractionSources;
    use crate::input::InputDeadZone;
    use crate::input::OrbitCamBlenderLikeKeyboardPreset;
    use crate::input::OrbitCamBlenderLikePreset;
    use crate::input::OrbitCamBlenderLikePresetDraft;
    use crate::input::OrbitCamGamepadPreset;
    use crate::input::OrbitCamInputAdapterPlugin;
    use crate::input::OrbitCamInputModeDescriptor;
    use crate::input::OrbitCamInputModeRejected;
    use crate::input::OrbitCamKeyboardPresetDraft;
    use crate::input::OrbitCamManualInputWriter;
    use crate::input::OrbitCamPresetDraft;
    use crate::input::OrbitCamPresetKind;
    use crate::input::OrbitCamRoutingPlugin;
    use crate::input::OrbitCamSensitivity;
    use crate::input::OrbitCamSensitivityDraft;
    use crate::input::bindings;
    use crate::system_sets::LagrangeSystemSetsPlugin;
    use crate::touch::TouchTracker;

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

    #[derive(Resource, Default)]
    struct DescriptorApplyEvents {
        applied:  usize,
        rejected: usize,
    }

    type TestResult = Result<(), &'static str>;

    const GAMEPAD_DEAD_ZONE_LOWER: f32 = 0.2;
    const GAMEPAD_DEAD_ZONE_UPPER: f32 = 0.9;
    const GAMEPAD_ORBIT_SCALE: f32 = 900.0;
    const GAMEPAD_ORBIT_SENSITIVITY: f32 = 0.75;
    const GAMEPAD_PAN_SCALE: f32 = 600.0;
    const GAMEPAD_PAN_SENSITIVITY: f32 = 0.5;
    const GAMEPAD_SLOW_ORBIT_SCALE: f32 = 90.0;
    const GAMEPAD_SLOW_PAN_SCALE: f32 = 60.0;
    const GAMEPAD_SLOW_ZOOM_SCALE: f32 = 0.4;
    const GAMEPAD_ZOOM_SCALE: f32 = 6.0;
    const GAMEPAD_ZOOM_SENSITIVITY: f32 = 0.25;
    const INVALID_SOURCE_SENSITIVITY: f32 = -1.0;
    const TUNED_MOUSE_ORBIT_SENSITIVITY: f32 = 0.25;
    const TUNED_MOUSE_PAN_SENSITIVITY: f32 = 0.5;
    const TUNED_MOUSE_ZOOM_SENSITIVITY: f32 = 0.75;
    const TUNED_SLOW_SCALE: f32 = 0.25;
    const TUNED_SMOOTH_ORBIT_SENSITIVITY: f32 = 2.0;
    const TUNED_SMOOTH_PAN_SENSITIVITY: f32 = 0.0;
    const TUNED_SMOOTH_ZOOM_SENSITIVITY: f32 = 3.0;

    fn test_app() -> App {
        let mut app = App::new();
        app.add_plugins((
            MinimalPlugins,
            LagrangeSystemSetsPlugin,
            OrbitCamInputModesPlugin,
        ));
        app
    }

    fn adapter_test_app() -> App {
        let mut app = App::new();
        app.add_plugins((
            MinimalPlugins,
            LagrangeEnhancedInputPlugin,
            LagrangeSystemSetsPlugin,
            OrbitCamInputModesPlugin,
            OrbitCamRoutingPlugin,
            OrbitCamInputAdapterPlugin,
        ));
        app.init_resource::<ButtonInput<KeyCode>>()
            .init_resource::<ButtonInput<MouseButton>>()
            .init_resource::<AccumulatedMouseMotion>()
            .init_resource::<AccumulatedMouseScroll>()
            .init_resource::<TouchTracker>()
            .add_message::<PinchGesture>();
        app.finish();
        app
    }

    fn invalid_blender_like_preset() -> OrbitCamPreset {
        OrbitCamBlenderLikePreset::default()
            .mouse_sensitivity(OrbitCamSensitivity::uniform(INVALID_SOURCE_SENSITIVITY))
            .into()
    }

    fn invalid_blender_like_draft() -> OrbitCamPresetDraft {
        OrbitCamPresetDraft::BlenderLike(OrbitCamBlenderLikePresetDraft {
            mouse_sensitivity:         OrbitCamSensitivityDraft {
                orbit: INVALID_SOURCE_SENSITIVITY,
                ..default()
            },
            smooth_scroll_sensitivity: OrbitCamSensitivityDraft::default(),
            zoom_mod_keys:             ModKeys::CONTROL,
            slow_toggle_key:           Some(KeyCode::KeyS),
            slow_toggle_mod_keys:      ModKeys::ALT,
            slow_scale:                TUNED_SLOW_SCALE,
        })
    }

    fn tuned_blender_like_draft() -> OrbitCamBlenderLikePresetDraft {
        OrbitCamBlenderLikePresetDraft {
            mouse_sensitivity:         OrbitCamSensitivityDraft {
                orbit: TUNED_MOUSE_ORBIT_SENSITIVITY,
                pan:   TUNED_MOUSE_PAN_SENSITIVITY,
                zoom:  TUNED_MOUSE_ZOOM_SENSITIVITY,
            },
            smooth_scroll_sensitivity: OrbitCamSensitivityDraft {
                orbit: TUNED_SMOOTH_ORBIT_SENSITIVITY,
                pan:   TUNED_SMOOTH_PAN_SENSITIVITY,
                zoom:  TUNED_SMOOTH_ZOOM_SENSITIVITY,
            },
            zoom_mod_keys:             ModKeys::ALT,
            slow_toggle_key:           Some(KeyCode::Space),
            slow_toggle_mod_keys:      ModKeys::SHIFT,
            slow_scale:                TUNED_SLOW_SCALE,
        }
    }

    fn tuned_blender_like_preset() -> OrbitCamBlenderLikePreset {
        OrbitCamBlenderLikePreset::default()
            .mouse_sensitivity(
                OrbitCamSensitivity::new()
                    .orbit(TUNED_MOUSE_ORBIT_SENSITIVITY)
                    .pan(TUNED_MOUSE_PAN_SENSITIVITY)
                    .zoom(TUNED_MOUSE_ZOOM_SENSITIVITY),
            )
            .smooth_scroll_sensitivity(
                OrbitCamSensitivity::new()
                    .orbit(TUNED_SMOOTH_ORBIT_SENSITIVITY)
                    .pan(TUNED_SMOOTH_PAN_SENSITIVITY)
                    .zoom(TUNED_SMOOTH_ZOOM_SENSITIVITY),
            )
            .zoom_mod_keys(ModKeys::ALT)
            .slow_toggle_key(Some(KeyCode::Space))
            .slow_toggle_mod_keys(ModKeys::SHIFT)
            .slow_scale(TUNED_SLOW_SCALE)
    }

    fn tuned_gamepad_preset() -> OrbitCamGamepadPreset {
        OrbitCamGamepadPreset::default()
            .gamepad_sensitivity(
                OrbitCamSensitivity::new()
                    .orbit(GAMEPAD_ORBIT_SENSITIVITY)
                    .pan(GAMEPAD_PAN_SENSITIVITY)
                    .zoom(GAMEPAD_ZOOM_SENSITIVITY),
            )
            .orbit_scale(GAMEPAD_ORBIT_SCALE)
            .slow_orbit_scale(GAMEPAD_SLOW_ORBIT_SCALE)
            .pan_scale(GAMEPAD_PAN_SCALE)
            .slow_pan_scale(GAMEPAD_SLOW_PAN_SCALE)
            .zoom_scale(GAMEPAD_ZOOM_SCALE)
            .slow_zoom_scale(GAMEPAD_SLOW_ZOOM_SCALE)
            .stick_dead_zone(InputDeadZone::new(
                GAMEPAD_DEAD_ZONE_LOWER,
                GAMEPAD_DEAD_ZONE_UPPER,
            ))
    }

    fn spawn_adapter_camera(app: &mut App, components: impl Bundle) -> Entity {
        app.world_mut()
            .spawn((
                OrbitCam::default(),
                OrbitCamInput::default(),
                Camera::default(),
                RenderTarget::Window(WindowRef::Primary),
                components,
            ))
            .id()
    }

    fn assert_f32_close(actual: f32, expected: f32) {
        assert!(
            (actual - expected).abs() <= f32::EPSILON,
            "expected {expected}, got {actual}",
        );
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
    fn descriptor_applies_preset_mode() {
        let mut app = test_app();
        let expected_preset = tuned_blender_like_preset();
        let camera = app
            .world_mut()
            .spawn((
                OrbitCam::default(),
                OrbitCamInput::default(),
                OrbitCamInputModeDescriptor {
                    mode: OrbitCamInputModeDraft::Preset(OrbitCamPresetDraft::BlenderLike(
                        tuned_blender_like_draft(),
                    )),
                },
            ))
            .id();

        app.update();

        assert_eq!(
            app.world().get::<OrbitCamInputMode>(camera),
            Some(&OrbitCamInputMode::with_preset(expected_preset))
        );
        assert!(matches!(
            app.world().get::<OrbitCamInputMode>(camera),
            Some(OrbitCamInputMode::Preset(_))
        ));
        assert_eq!(
            app.world()
                .get::<OrbitCamInputModeApplyStatus>(camera)
                .map(|status| status.state),
            Some(OrbitCamInputModeApplyState::Applied)
        );
    }

    #[test]
    fn reflected_preset_draft_constructs_tuned_preset_without_fluent_setters() -> TestResult {
        let preset =
            OrbitCamPreset::try_from(OrbitCamPresetDraft::BlenderLike(tuned_blender_like_draft()))
                .map_err(|_| "tuned reflected preset draft should validate")?;

        assert_eq!(preset, tuned_blender_like_preset().into());
        Ok(())
    }

    #[test]
    fn descriptor_applies_custom_bindings_mode() {
        let mut app = test_app();
        let camera = app
            .world_mut()
            .spawn((
                OrbitCam::default(),
                OrbitCamInput::default(),
                OrbitCamInputModeDescriptor {
                    mode: OrbitCamInputModeDraft::Bindings(OrbitCamBindingsDescriptor::default()),
                },
            ))
            .id();

        app.update();

        assert!(matches!(
            app.world().get::<OrbitCamInputMode>(camera),
            Some(OrbitCamInputMode::Bindings(_))
        ));
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
                OrbitCamInputMode::with_preset(OrbitCamPreset::blender_like()),
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
            Some(&OrbitCamInputMode::with_preset(
                OrbitCamPreset::blender_like()
            ))
        );
        assert_eq!(
            app.world()
                .get::<OrbitCamInputModeApplyStatus>(camera)
                .map(|status| status.state),
            Some(OrbitCamInputModeApplyState::Rejected)
        );
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

    #[test]
    fn invalid_reflected_preset_apply_keeps_previous_runtime_state() -> TestResult {
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
        app.init_resource::<DescriptorApplyEvents>();
        app.world_mut().entity_mut(camera).observe(
            |_replaced: On<OrbitCamInputModeReplaced>,
             mut events: ResMut<ModeReplacementEvents>| {
                events.0 += 1;
            },
        );
        app.world_mut().entity_mut(camera).observe(
            |_applied: On<OrbitCamInputModeApplied>, mut events: ResMut<DescriptorApplyEvents>| {
                events.applied += 1;
            },
        );
        app.world_mut().entity_mut(camera).observe(
            |_rejected: On<OrbitCamInputModeRejected>,
             mut events: ResMut<DescriptorApplyEvents>| {
                events.rejected += 1;
            },
        );

        app.world_mut()
            .entity_mut(camera)
            .insert(OrbitCamInputModeDescriptor {
                mode: OrbitCamInputModeDraft::Preset(invalid_blender_like_draft()),
            });
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
        assert_eq!(
            app.world()
                .get::<OrbitCamInputModeApplyStatus>(camera)
                .map(|status| status.state),
            Some(OrbitCamInputModeApplyState::Rejected)
        );
        let descriptor_events = app.world().resource::<DescriptorApplyEvents>();
        assert_eq!(descriptor_events.applied, 0);
        assert_eq!(descriptor_events.rejected, 1);

        Ok(())
    }

    #[test]
    fn invalid_reflected_preset_apply_preserves_installed_entities() -> TestResult {
        let mut app = adapter_test_app();
        let valid_mode = OrbitCamInputMode::with_preset(OrbitCamPreset::gamepad());
        let camera = spawn_adapter_camera(&mut app, valid_mode.clone());
        app.insert_resource(CameraInputRoutingConfig::explicit(camera));
        app.update();

        let previous_entities = installed_input_entities(app.world(), camera);
        if previous_entities.len() <= 1 {
            return Err("adapter should replace placeholder with action entities");
        }
        app.world_mut()
            .entity_mut(camera)
            .insert(OrbitCamInputModeDescriptor {
                mode: OrbitCamInputModeDraft::Preset(invalid_blender_like_draft()),
            });
        app.update();

        assert_eq!(
            app.world().get::<OrbitCamInputMode>(camera),
            Some(&valid_mode)
        );
        assert_eq!(
            app.world()
                .get::<OrbitCamInputModeApplyStatus>(camera)
                .map(|status| status.state),
            Some(OrbitCamInputModeApplyState::Rejected)
        );
        assert_eq!(
            installed_input_entities(app.world(), camera),
            previous_entities
        );

        Ok(())
    }

    #[test]
    fn stale_descriptor_does_not_reapply_after_direct_mode_change() {
        let mut app = test_app();
        let direct_mode = OrbitCamInputMode::with_preset(OrbitCamPreset::gamepad());
        let camera = app
            .world_mut()
            .spawn((
                OrbitCam::default(),
                OrbitCamInput::default(),
                OrbitCamInputModeDescriptor {
                    mode: OrbitCamInputModeDraft::from_preset(&OrbitCamPreset::simple_mouse()),
                },
            ))
            .id();

        app.update();
        app.world_mut()
            .entity_mut(camera)
            .insert(direct_mode.clone());
        app.update();
        app.update();

        assert_eq!(
            app.world().get::<OrbitCamInputMode>(camera),
            Some(&direct_mode)
        );
    }

    #[test]
    fn manual_export_helper_returns_manual_draft() {
        assert_eq!(
            OrbitCamInputModeDraft::manual(),
            OrbitCamInputModeDraft::Manual
        );
    }

    #[test]
    fn preset_export_preserves_tuned_blender_like_keyboard_payload() -> TestResult {
        let preset =
            OrbitCamBlenderLikeKeyboardPreset::default().blender_like(tuned_blender_like_preset());
        let mode = OrbitCamInputMode::with_preset(preset);
        let OrbitCamInputMode::Preset(runtime_preset) = &mode else {
            return Err("with_preset should construct preset mode");
        };
        let draft = OrbitCamInputModeDraft::from_preset(runtime_preset);

        assert_eq!(
            input::describe_orbit_cam_controls(&mode).mode_value,
            OrbitCamPresetKind::BlenderLikeKeyboard.name()
        );
        match draft {
            OrbitCamInputModeDraft::Preset(OrbitCamPresetDraft::BlenderLikeKeyboard(draft)) => {
                assert_eq!(draft.pointer, tuned_blender_like_draft());
                assert_eq!(draft.keyboard, OrbitCamKeyboardPresetDraft);
            },
            _ => return Err("preset export should preserve BlenderLikeKeyboard identity"),
        }
        Ok(())
    }

    #[test]
    fn preset_export_preserves_tuned_gamepad_payload() -> TestResult {
        let mode = OrbitCamInputMode::with_preset(tuned_gamepad_preset());
        let OrbitCamInputMode::Preset(runtime_preset) = &mode else {
            return Err("with_preset should construct preset mode");
        };
        let draft = OrbitCamInputModeDraft::from_preset(runtime_preset);

        assert_eq!(
            input::describe_orbit_cam_controls(&mode).mode_value,
            OrbitCamPresetKind::Gamepad.name()
        );
        match draft {
            OrbitCamInputModeDraft::Preset(OrbitCamPresetDraft::Gamepad(draft)) => {
                assert_eq!(
                    draft.gamepad_sensitivity,
                    OrbitCamSensitivityDraft {
                        orbit: GAMEPAD_ORBIT_SENSITIVITY,
                        pan:   GAMEPAD_PAN_SENSITIVITY,
                        zoom:  GAMEPAD_ZOOM_SENSITIVITY,
                    }
                );
                assert_f32_close(draft.orbit_scale, GAMEPAD_ORBIT_SCALE);
                assert_f32_close(draft.slow_orbit_scale, GAMEPAD_SLOW_ORBIT_SCALE);
                assert_f32_close(draft.pan_scale, GAMEPAD_PAN_SCALE);
                assert_f32_close(draft.slow_pan_scale, GAMEPAD_SLOW_PAN_SCALE);
                assert_f32_close(draft.zoom_scale, GAMEPAD_ZOOM_SCALE);
                assert_f32_close(draft.slow_zoom_scale, GAMEPAD_SLOW_ZOOM_SCALE);
                assert_f32_close(
                    draft.stick_dead_zone.lower_threshold,
                    GAMEPAD_DEAD_ZONE_LOWER,
                );
                assert_f32_close(
                    draft.stick_dead_zone.upper_threshold,
                    GAMEPAD_DEAD_ZONE_UPPER,
                );
            },
            _ => return Err("preset export should preserve Gamepad identity"),
        }
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
