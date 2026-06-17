//! Installation, teardown, and context-gating for the enhanced-input adapter pipeline.
//!
//! Types (all `pub(super)` so siblings in `adapter/` can see them):
//! - [`OrbitCamInstalledBindings`] — component snapshotting the [`OrbitCamBindings`] used to
//!   install enhanced-input entities on a camera.
//! - [`OrbitCamInputActionEntities`] — component holding the `Entity` handles of every per-camera
//!   action plus the per-action source masks resolved at install time.
//! - [`OrbitCamAdapterDiagnostics`] — resource updated each frame with counts of installed cameras
//!   / installed entities / gated cameras.
//! - [`SpawnedInputInstallation`] — file-local return type of [`spawn_input_installation`].
//!
//! Systems (registered in [`OrbitCamInputInternalSet::Installation`]):
//! - [`clear_replaced_or_manual_installations`] — strips enhanced-input components when the
//!   camera's input mode was replaced or has a placeholder marker.
//! - [`install_enhanced_input_entities`] — for each camera with a placeholder, builds the
//!   `OrbitCamBindings`, spawns the action and binding entities, and inserts the components in this
//!   file.
//! - [`apply_context_gating`] — flips each camera's [`ContextActivity<OrbitCamInputContext>`] based
//!   on its [`OrbitCamInputContextGated`] state.

use std::collections::HashMap;

use bevy::prelude::*;
use bevy_enhanced_input::prelude::Accumulation;
use bevy_enhanced_input::prelude::Action;
use bevy_enhanced_input::prelude::ActionOf;
use bevy_enhanced_input::prelude::ActionSettings;
use bevy_enhanced_input::prelude::ActionValue;
use bevy_enhanced_input::prelude::Actions;
use bevy_enhanced_input::prelude::ActionsQuery;
use bevy_enhanced_input::prelude::Binding;
use bevy_enhanced_input::prelude::BindingOf;
use bevy_enhanced_input::prelude::ConditionKind;
use bevy_enhanced_input::prelude::ContextActivity;
use bevy_enhanced_input::prelude::ContextPriority;
use bevy_enhanced_input::prelude::ContextTime;
use bevy_enhanced_input::prelude::CustomInput;
use bevy_enhanced_input::prelude::CustomInputs;
use bevy_enhanced_input::prelude::DeadZone;
use bevy_enhanced_input::prelude::DeadZoneKind;
use bevy_enhanced_input::prelude::DeltaScale;
use bevy_enhanced_input::prelude::GamepadDevice;
use bevy_enhanced_input::prelude::InputAction;
use bevy_enhanced_input::prelude::InputCondition;
use bevy_enhanced_input::prelude::ModKeys;
use bevy_enhanced_input::prelude::Negate;
use bevy_enhanced_input::prelude::Press;
use bevy_enhanced_input::prelude::Scale;
use bevy_enhanced_input::prelude::SwizzleAxis;
use bevy_enhanced_input::prelude::TriggerState;

use super::inject::OrbitCamAdapterFrameSources;
use super::inject::TrackpadScrollTarget;
use crate::constants::PIXEL_SCROLL_SCALE;
use crate::input::ActionBindingEntry;
use crate::input::BindingGates;
use crate::input::CameraInputGamepadSelectionPolicy;
use crate::input::CameraInteractionSources;
use crate::input::CameraSemanticAction;
use crate::input::ControlSpeed;
use crate::input::HeldActionBindingEntry;
use crate::input::HeldCameraAction;
use crate::input::InputAxisTransform;
use crate::input::InputBindingDescriptor;
use crate::input::InputBindingModifiers;
use crate::input::InputDeadZone;
use crate::input::InputDeltaScale;
use crate::input::OrbitCamBindings;
use crate::input::OrbitCamGateInput;
use crate::input::OrbitCamGatePolarity;
use crate::input::OrbitCamInputContext;
use crate::input::OrbitCamInputContextGated;
use crate::input::OrbitCamOrbitAction;
use crate::input::OrbitCamPanAction;
use crate::input::OrbitCamResolvedBindings;
use crate::input::OrbitCamTrackpadScroll;
use crate::input::OrbitCamZoomCoarseAction;
use crate::input::OrbitCamZoomSmoothAction;
use crate::input::ZoomInversion;
use crate::input::actions::OrbitCamAdapterOrbitAction;
use crate::input::actions::OrbitCamAdapterPanAction;
use crate::input::actions::OrbitCamAdapterZoomCoarseAction;
use crate::input::actions::OrbitCamAdapterZoomSmoothAction;
use crate::input::actions::OrbitCamGateAction;
use crate::input::actions::OrbitCamOrbitEngagedAction;
use crate::input::actions::OrbitCamOrbitSlowAction;
use crate::input::actions::OrbitCamPanEngagedAction;
use crate::input::actions::OrbitCamPanSlowAction;
use crate::input::actions::OrbitCamSlowModeToggleAction;
use crate::input::actions::OrbitCamZoomEngagedAction;
use crate::input::actions::OrbitCamZoomSmoothSlowAction;
use crate::input::modes;
use crate::input::modes::OrbitCamInputInstallationOf;
use crate::orbit_cam::OrbitCam;

#[derive(Component, Clone, Debug, PartialEq)]
pub(super) struct OrbitCamInstalledBindings(pub(super) OrbitCamBindings);

#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct OrbitCamInputActionEntities {
    pub(super) orbit:               Entity,
    pub(super) orbit_slow:          Entity,
    pub(super) orbit_engaged:       Entity,
    pub(super) pan:                 Entity,
    pub(super) pan_slow:            Entity,
    pub(super) pan_engaged:         Entity,
    pub(super) zoom_coarse:         Entity,
    pub(super) zoom_smooth:         Entity,
    pub(super) zoom_smooth_slow:    Entity,
    pub(super) zoom_engaged:        Entity,
    pub(super) slow_mode_toggle:    Entity,
    pub(super) adapter_orbit:       Entity,
    pub(super) adapter_pan:         Entity,
    pub(super) adapter_zoom_coarse: Entity,
    pub(super) adapter_zoom_smooth: Entity,
    pub(super) orbit_sources:       CameraInteractionSources,
    pub(super) pan_sources:         CameraInteractionSources,
    pub(super) zoom_coarse_sources: CameraInteractionSources,
    pub(super) zoom_smooth_sources: CameraInteractionSources,
}

#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct OrbitCamAdapterCustomInputs {
    pub(super) orbit:       CustomInput,
    pub(super) pan:         CustomInput,
    pub(super) trackpad:    CustomInput,
    pub(super) zoom_coarse: CustomInput,
    pub(super) zoom_smooth: CustomInput,
}

#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct TrackpadBindingCondition {
    pub(super) target:   TrackpadScrollTarget,
    pub(super) mod_keys: ModKeys,
    pub(super) active:   bool,
}

impl TrackpadBindingCondition {
    const fn new(target: TrackpadScrollTarget, binding: OrbitCamTrackpadScroll) -> Self {
        Self {
            target,
            mod_keys: binding.mod_keys,
            active: false,
        }
    }
}

impl InputCondition for TrackpadBindingCondition {
    fn evaluate(
        &mut self,
        _actions: &ActionsQuery,
        _time: &ContextTime,
        value: ActionValue,
    ) -> TriggerState {
        if self.active && value.as_bool() {
            TriggerState::Fired
        } else {
            TriggerState::None
        }
    }
}

#[derive(Component, Clone, Debug, PartialEq, Eq)]
pub(super) struct OrbitCamBindingGateCondition {
    pub(super) gates: Vec<InstalledBindingGate>,
}

impl OrbitCamBindingGateCondition {
    fn new(gates: impl Into<Vec<InstalledBindingGate>>) -> Self {
        Self {
            gates: gates.into(),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct InstalledBindingGate {
    action:   Entity,
    polarity: OrbitCamGatePolarity,
}

impl InputCondition for OrbitCamBindingGateCondition {
    fn evaluate(
        &mut self,
        actions: &ActionsQuery,
        _time: &ContextTime,
        value: ActionValue,
    ) -> TriggerState {
        let actuated = value.as_bool();
        if !actuated {
            return TriggerState::None;
        }

        let gates_satisfied = self.gates.iter().all(|gate| {
            let active = actions.get(gate.action).is_ok_and(|(_, state, ..)| {
                matches!(*state, TriggerState::Ongoing | TriggerState::Fired)
            });
            match gate.polarity {
                OrbitCamGatePolarity::Required => active,
                OrbitCamGatePolarity::Blocked => !active,
            }
        });
        if gates_satisfied {
            TriggerState::Fired
        } else {
            TriggerState::None
        }
    }

    fn kind(&self) -> ConditionKind { ConditionKind::Implicit }
}

#[derive(Resource, Clone, Debug, Default, PartialEq, Eq)]
pub(super) struct OrbitCamAdapterDiagnostics {
    pub(super) installed_cameras:  usize,
    pub(super) installed_entities: usize,
    pub(super) gated_cameras:      usize,
}

pub(super) fn clear_replaced_or_manual_installations(world: &mut World) {
    let mut query = world.query_filtered::<Entity, With<OrbitCamInputContext>>();
    let cameras = query.iter(world).collect::<Vec<_>>();

    for camera in cameras {
        let installed_entities = modes::installed_input_entities(world, camera);
        if installed_entities.is_empty() || modes::input_installation_has_placeholder(world, camera)
        {
            clear_enhanced_input_components(world, camera);
        }
    }
}

fn clear_enhanced_input_components(world: &mut World, camera: Entity) {
    let mut entity = world.entity_mut(camera);
    entity
        .remove::<OrbitCamInputContext>()
        .remove::<ContextActivity<OrbitCamInputContext>>()
        .remove::<ContextPriority<OrbitCamInputContext>>()
        .remove::<GamepadDevice>()
        .remove::<Actions<OrbitCamInputContext>>()
        .remove::<OrbitCamInputActionEntities>()
        .remove::<OrbitCamInstalledBindings>()
        .remove::<OrbitCamAdapterCustomInputs>()
        .remove::<OrbitCamAdapterFrameSources>();
}

pub(super) fn install_enhanced_input_entities(world: &mut World) {
    let mut query = world.query_filtered::<(Entity, &OrbitCamResolvedBindings), With<OrbitCam>>();
    let cameras = query
        .iter(world)
        .filter(|(camera, _)| modes::input_installation_has_placeholder(world, *camera))
        .map(|(camera, bindings)| (camera, bindings.0.clone()))
        .collect::<Vec<_>>();

    let mut installed_cameras = 0;
    let mut installed_entities = 0;

    for (camera, bindings) in cameras {
        for installed_entity in modes::installed_input_entities(world, camera) {
            let _ = world.despawn(installed_entity);
        }

        let installation = spawn_input_installation(world, camera, &bindings);
        installed_entities += installation.entities.len();
        installed_cameras += 1;

        world.entity_mut(camera).insert((
            OrbitCamInputContext,
            gamepad_device_for(&bindings),
            OrbitCamInstalledBindings(bindings),
            installation.actions,
            installation.custom_inputs,
            OrbitCamAdapterFrameSources::default(),
        ));
        modes::replace_installed_input_entities(world, camera, installation.entities);
    }

    let mut diagnostics = world.resource_mut::<OrbitCamAdapterDiagnostics>();
    diagnostics.installed_cameras = installed_cameras;
    diagnostics.installed_entities = installed_entities;
}

const fn gamepad_device_for(bindings: &OrbitCamBindings) -> GamepadDevice {
    match bindings.gamepad() {
        CameraInputGamepadSelectionPolicy::Disabled => GamepadDevice::None,
        CameraInputGamepadSelectionPolicy::Active => GamepadDevice::Any,
    }
}

struct SpawnedInputInstallation {
    actions:       OrbitCamInputActionEntities,
    custom_inputs: OrbitCamAdapterCustomInputs,
    entities:      Vec<Entity>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct SpawnedInputActions {
    orbit:               Entity,
    orbit_slow:          Entity,
    orbit_engaged:       Entity,
    pan:                 Entity,
    pan_slow:            Entity,
    pan_engaged:         Entity,
    zoom_coarse:         Entity,
    zoom_smooth:         Entity,
    zoom_smooth_slow:    Entity,
    zoom_engaged:        Entity,
    slow_mode_toggle:    Entity,
    adapter_orbit:       Entity,
    adapter_pan:         Entity,
    adapter_zoom_coarse: Entity,
    adapter_zoom_smooth: Entity,
}

impl SpawnedInputActions {
    fn entities(self) -> Vec<Entity> {
        vec![
            self.orbit,
            self.orbit_slow,
            self.orbit_engaged,
            self.pan,
            self.pan_slow,
            self.pan_engaged,
            self.zoom_coarse,
            self.zoom_smooth,
            self.zoom_smooth_slow,
            self.zoom_engaged,
            self.slow_mode_toggle,
            self.adapter_orbit,
            self.adapter_pan,
            self.adapter_zoom_coarse,
            self.adapter_zoom_smooth,
        ]
    }

    const fn adapter_actions(self) -> (Entity, Entity, Entity, Entity) {
        (
            self.adapter_orbit,
            self.adapter_pan,
            self.adapter_zoom_coarse,
            self.adapter_zoom_smooth,
        )
    }

    const fn trackpad_actions(self) -> (Entity, Entity, Entity) {
        (
            self.adapter_orbit,
            self.adapter_pan,
            self.adapter_zoom_smooth,
        )
    }

    fn with_sources(self, bindings: &OrbitCamBindings) -> OrbitCamInputActionEntities {
        OrbitCamInputActionEntities {
            orbit:               self.orbit,
            orbit_slow:          self.orbit_slow,
            orbit_engaged:       self.orbit_engaged,
            pan:                 self.pan,
            pan_slow:            self.pan_slow,
            pan_engaged:         self.pan_engaged,
            zoom_coarse:         self.zoom_coarse,
            zoom_smooth:         self.zoom_smooth,
            zoom_smooth_slow:    self.zoom_smooth_slow,
            zoom_engaged:        self.zoom_engaged,
            slow_mode_toggle:    self.slow_mode_toggle,
            adapter_orbit:       self.adapter_orbit,
            adapter_pan:         self.adapter_pan,
            adapter_zoom_coarse: self.adapter_zoom_coarse,
            adapter_zoom_smooth: self.adapter_zoom_smooth,
            orbit_sources:       held_sources(bindings.orbit().entries()),
            pan_sources:         held_sources(bindings.pan().entries()),
            zoom_coarse_sources: action_sources(bindings.zoom_coarse().entries()),
            zoom_smooth_sources: held_sources(bindings.zoom_smooth().entries()),
        }
    }
}

#[derive(Default)]
struct GateActionCache {
    actions: HashMap<OrbitCamGateInput, Entity>,
}

fn spawn_input_installation(
    world: &mut World,
    camera: Entity,
    bindings: &OrbitCamBindings,
) -> SpawnedInputInstallation {
    let custom_inputs = register_adapter_custom_inputs(world);
    let actions = spawn_input_actions(world, camera);
    let mut entities = actions.entities();
    spawn_adapter_custom_bindings(
        world,
        camera,
        &custom_inputs,
        actions.adapter_actions(),
        &mut entities,
    );
    spawn_trackpad_custom_bindings(
        world,
        camera,
        &custom_inputs,
        actions.trackpad_actions(),
        bindings,
        &mut entities,
    );
    spawn_camera_action_bindings(world, camera, bindings, actions, &mut entities);

    SpawnedInputInstallation {
        actions: actions.with_sources(bindings),
        custom_inputs,
        entities,
    }
}

fn spawn_input_actions(world: &mut World, camera: Entity) -> SpawnedInputActions {
    SpawnedInputActions {
        orbit:               spawn_action::<OrbitCamOrbitAction>(world, camera),
        orbit_slow:          spawn_action::<OrbitCamOrbitSlowAction>(world, camera),
        orbit_engaged:       spawn_action::<OrbitCamOrbitEngagedAction>(world, camera),
        pan:                 spawn_action::<OrbitCamPanAction>(world, camera),
        pan_slow:            spawn_action::<OrbitCamPanSlowAction>(world, camera),
        pan_engaged:         spawn_action::<OrbitCamPanEngagedAction>(world, camera),
        zoom_coarse:         spawn_action::<OrbitCamZoomCoarseAction>(world, camera),
        zoom_smooth:         spawn_action::<OrbitCamZoomSmoothAction>(world, camera),
        zoom_smooth_slow:    spawn_action::<OrbitCamZoomSmoothSlowAction>(world, camera),
        zoom_engaged:        spawn_action::<OrbitCamZoomEngagedAction>(world, camera),
        slow_mode_toggle:    spawn_action::<OrbitCamSlowModeToggleAction>(world, camera),
        adapter_orbit:       spawn_action::<OrbitCamAdapterOrbitAction>(world, camera),
        adapter_pan:         spawn_action::<OrbitCamAdapterPanAction>(world, camera),
        adapter_zoom_coarse: spawn_action::<OrbitCamAdapterZoomCoarseAction>(world, camera),
        adapter_zoom_smooth: spawn_action::<OrbitCamAdapterZoomSmoothAction>(world, camera),
    }
}

fn spawn_camera_action_bindings(
    world: &mut World,
    camera: Entity,
    bindings: &OrbitCamBindings,
    actions: SpawnedInputActions,
    entities: &mut Vec<Entity>,
) {
    let mut gate_actions = GateActionCache::default();
    spawn_held_bindings(
        world,
        camera,
        MotionActions {
            normal: actions.orbit,
            slow:   actions.orbit_slow,
        },
        actions.orbit_engaged,
        bindings.orbit().entries(),
        &mut gate_actions,
        entities,
    );
    spawn_held_bindings(
        world,
        camera,
        MotionActions {
            normal: actions.pan,
            slow:   actions.pan_slow,
        },
        actions.pan_engaged,
        bindings.pan().entries(),
        &mut gate_actions,
        entities,
    );
    spawn_held_bindings(
        world,
        camera,
        MotionActions {
            normal: actions.zoom_smooth,
            slow:   actions.zoom_smooth_slow,
        },
        actions.zoom_engaged,
        bindings.zoom_smooth().entries(),
        &mut gate_actions,
        entities,
    );
    for entry in bindings.zoom_coarse().entries() {
        spawn_binding(
            world,
            camera,
            actions.zoom_coarse,
            entry.binding_descriptor(),
            &BindingGates::default(),
            &mut gate_actions,
            entities,
        );
    }
    if let Some(slow_mode) = bindings.slow_mode() {
        world
            .entity_mut(actions.slow_mode_toggle)
            .insert(Press::default());
        let entity = spawn_single_binding(
            world,
            actions.slow_mode_toggle,
            OrbitCamInputInstallationOf(camera),
            Binding::Keyboard {
                key:      slow_mode.toggle_key,
                mod_keys: slow_mode.mod_keys,
            },
        );
        entities.push(entity);
    }
}

fn register_adapter_custom_inputs(world: &mut World) -> OrbitCamAdapterCustomInputs {
    let mut custom_inputs = world.resource_mut::<CustomInputs>();
    OrbitCamAdapterCustomInputs {
        orbit:       custom_inputs.register_input(),
        pan:         custom_inputs.register_input(),
        trackpad:    custom_inputs.register_input(),
        zoom_coarse: custom_inputs.register_input(),
        zoom_smooth: custom_inputs.register_input(),
    }
}

fn spawn_adapter_custom_bindings(
    world: &mut World,
    camera: Entity,
    custom_inputs: &OrbitCamAdapterCustomInputs,
    actions: (Entity, Entity, Entity, Entity),
    entities: &mut Vec<Entity>,
) {
    let installation = OrbitCamInputInstallationOf(camera);
    let (orbit, pan, zoom_coarse, zoom_smooth) = actions;
    entities.push(spawn_single_binding(
        world,
        orbit,
        installation,
        Binding::Custom(custom_inputs.orbit),
    ));
    entities.push(spawn_single_binding(
        world,
        pan,
        installation,
        Binding::Custom(custom_inputs.pan),
    ));
    entities.push(spawn_single_binding(
        world,
        zoom_coarse,
        installation,
        Binding::Custom(custom_inputs.zoom_coarse),
    ));
    entities.push(spawn_single_binding(
        world,
        zoom_smooth,
        installation,
        Binding::Custom(custom_inputs.zoom_smooth),
    ));
}

fn spawn_trackpad_custom_bindings(
    world: &mut World,
    camera: Entity,
    custom_inputs: &OrbitCamAdapterCustomInputs,
    actions: (Entity, Entity, Entity),
    bindings: &OrbitCamBindings,
    entities: &mut Vec<Entity>,
) {
    let (orbit, pan, zoom_smooth) = actions;
    for binding in bindings.trackpad_orbit() {
        entities.push(spawn_trackpad_binding(
            world,
            camera,
            orbit,
            custom_inputs.trackpad,
            TrackpadScrollTarget::Orbit,
            *binding,
        ));
    }
    for binding in bindings.trackpad_pan() {
        entities.push(spawn_trackpad_binding(
            world,
            camera,
            pan,
            custom_inputs.trackpad,
            TrackpadScrollTarget::Pan,
            *binding,
        ));
    }
    for binding in bindings.trackpad_zoom() {
        entities.push(spawn_trackpad_zoom_binding(
            world,
            camera,
            zoom_smooth,
            custom_inputs.trackpad,
            *binding,
            bindings.zoom_inversion(),
        ));
    }
}

/// Spawns a trackpad scroll binding whose two scroll axes drive orbit or pan
/// directly, with no zoom conversion.
fn spawn_trackpad_binding(
    world: &mut World,
    camera: Entity,
    action: Entity,
    input: CustomInput,
    target: TrackpadScrollTarget,
    binding: OrbitCamTrackpadScroll,
) -> Entity {
    let entity = spawn_single_binding(
        world,
        action,
        OrbitCamInputInstallationOf(camera),
        Binding::Custom(input),
    );
    insert_trackpad_condition(world, entity, target, binding)
}

/// Spawns a trackpad scroll binding that drives zoom: the vertical scroll axis
/// is swizzled onto the zoom axis and scaled, then negated when the camera's
/// zoom is inverted.
fn spawn_trackpad_zoom_binding(
    world: &mut World,
    camera: Entity,
    action: Entity,
    input: CustomInput,
    binding: OrbitCamTrackpadScroll,
    zoom_inversion: ZoomInversion,
) -> Entity {
    let installation = OrbitCamInputInstallationOf(camera);
    let entity = spawn_single_binding(world, action, installation, Binding::Custom(input));
    world
        .entity_mut(entity)
        .insert((SwizzleAxis::YXZ, Scale::splat(PIXEL_SCROLL_SCALE)));
    if matches!(zoom_inversion, ZoomInversion::Inverted) {
        world.entity_mut(entity).insert(Negate::all());
    }
    insert_trackpad_condition(world, entity, TrackpadScrollTarget::Zoom, binding)
}

fn insert_trackpad_condition(
    world: &mut World,
    entity: Entity,
    target: TrackpadScrollTarget,
    binding: OrbitCamTrackpadScroll,
) -> Entity {
    world
        .entity_mut(entity)
        .insert(TrackpadBindingCondition::new(target, binding));
    entity
}

fn spawn_action<A: InputAction>(world: &mut World, camera: Entity) -> Entity {
    world
        .spawn((
            Action::<A>::new(),
            ActionOf::<OrbitCamInputContext>::new(camera),
            action_settings(),
            OrbitCamInputInstallationOf(camera),
        ))
        .id()
}

const fn action_settings() -> ActionSettings {
    ActionSettings {
        accumulation:  Accumulation::Cumulative,
        require_reset: false,
        consume_input: false,
    }
}

fn spawn_held_bindings<A: HeldCameraAction>(
    world: &mut World,
    camera: Entity,
    motion_actions: MotionActions,
    engagement_action: Entity,
    entries: &[HeldActionBindingEntry<A>],
    gate_actions: &mut GateActionCache,
    entities: &mut Vec<Entity>,
) {
    for entry in entries {
        // Route the motion binding to the speed-specific action so BEI's gate
        // conditions decide which one fires; the active speed then falls out of
        // which motion action is firing — no gate logic is re-derived downstream.
        spawn_binding(
            world,
            camera,
            motion_actions.for_speed(entry.speed()),
            entry.motion_descriptor(),
            entry.gates(),
            gate_actions,
            entities,
        );
        spawn_binding(
            world,
            camera,
            engagement_action,
            entry.engagement_descriptor(),
            entry.gates(),
            gate_actions,
            entities,
        );
    }
}

/// The normal/slow motion action pair a motion binding routes to by speed.
#[derive(Clone, Copy)]
struct MotionActions {
    normal: Entity,
    slow:   Entity,
}

impl MotionActions {
    const fn for_speed(self, speed: ControlSpeed) -> Entity {
        match speed {
            ControlSpeed::Normal => self.normal,
            ControlSpeed::Slow => self.slow,
        }
    }
}

fn spawn_binding(
    world: &mut World,
    camera: Entity,
    action: Entity,
    binding_descriptor: &InputBindingDescriptor,
    gates: &BindingGates,
    gate_actions: &mut GateActionCache,
    entities: &mut Vec<Entity>,
) {
    let installation = OrbitCamInputInstallationOf(camera);
    let gate_entities = gate_condition_entities(world, camera, gates, gate_actions, entities);
    for entry in binding_descriptor.entries_slice() {
        let binding = spawn_binding_entry(
            world,
            action,
            installation,
            entry.binding(),
            entry.modifiers(),
        );
        if !gate_entities.is_empty() {
            world
                .entity_mut(binding)
                .insert(OrbitCamBindingGateCondition::new(gate_entities.clone()));
        }
        entities.push(binding);
    }
}

fn spawn_binding_entry(
    world: &mut World,
    action: Entity,
    installation: OrbitCamInputInstallationOf,
    binding: Binding,
    modifiers: InputBindingModifiers,
) -> Entity {
    let entity = spawn_single_binding(world, action, installation, binding);
    if let Some(dead_zone) = modifiers.dead_zone() {
        world
            .entity_mut(entity)
            .insert(dead_zone_modifier(dead_zone));
    }
    if let Some(scale) = modifiers.scale() {
        world.entity_mut(entity).insert(Scale::splat(scale));
    }
    if modifiers.delta_scale() == InputDeltaScale::Auto {
        world.entity_mut(entity).insert(DeltaScale::AUTO);
    }
    match modifiers.axis_transform() {
        InputAxisTransform::None => {},
        InputAxisTransform::Negate => {
            world.entity_mut(entity).insert(Negate::all());
        },
        InputAxisTransform::Swizzle => {
            world.entity_mut(entity).insert(SwizzleAxis::YXZ);
        },
        InputAxisTransform::SwizzleNegate => {
            world
                .entity_mut(entity)
                .insert((SwizzleAxis::YXZ, Negate::all()));
        },
    }
    entity
}

fn spawn_single_binding(
    world: &mut World,
    action: Entity,
    installation: OrbitCamInputInstallationOf,
    binding: Binding,
) -> Entity {
    world.spawn((binding, BindingOf(action), installation)).id()
}

const fn dead_zone_modifier(dead_zone: InputDeadZone) -> DeadZone {
    DeadZone {
        kind:            DeadZoneKind::Axial,
        lower_threshold: dead_zone.lower_threshold,
        upper_threshold: dead_zone.upper_threshold,
    }
}

fn gate_condition_entities(
    world: &mut World,
    camera: Entity,
    gates: &BindingGates,
    gate_actions: &mut GateActionCache,
    entities: &mut Vec<Entity>,
) -> Vec<InstalledBindingGate> {
    gates
        .entries()
        .iter()
        .map(|gate| InstalledBindingGate {
            action:   gate_action_entity(world, camera, gate.input, gate_actions, entities),
            polarity: gate.polarity,
        })
        .collect()
}

fn gate_action_entity(
    world: &mut World,
    camera: Entity,
    input: OrbitCamGateInput,
    gate_actions: &mut GateActionCache,
    entities: &mut Vec<Entity>,
) -> Entity {
    if let Some(action) = gate_actions.actions.get(&input) {
        return *action;
    }

    let action = spawn_action::<OrbitCamGateAction>(world, camera);
    let binding = spawn_single_binding(
        world,
        action,
        OrbitCamInputInstallationOf(camera),
        binding_for_gate_input(input),
    );
    entities.push(action);
    entities.push(binding);
    gate_actions.actions.insert(input, action);
    action
}

const fn binding_for_gate_input(input: OrbitCamGateInput) -> Binding {
    match input {
        OrbitCamGateInput::GamepadButton(button) => Binding::GamepadButton(button),
        OrbitCamGateInput::Key(key) => Binding::Keyboard {
            key,
            mod_keys: ModKeys::empty(),
        },
    }
}

fn held_sources<A: HeldCameraAction>(
    entries: &[HeldActionBindingEntry<A>],
) -> CameraInteractionSources {
    entries
        .iter()
        .fold(CameraInteractionSources::NONE, |sources, entry| {
            sources.union(entry.sources())
        })
}

fn action_sources<A>(entries: &[ActionBindingEntry<A>]) -> CameraInteractionSources
where
    A: CameraSemanticAction,
{
    entries
        .iter()
        .fold(CameraInteractionSources::NONE, |sources, entry| {
            sources.union(entry.sources())
        })
}

pub(super) fn apply_context_gating(world: &mut World) {
    let mut query = world.query_filtered::<(
        Entity,
        &OrbitCamInputContextGated,
        Option<&ContextActivity<OrbitCamInputContext>>,
    ), With<OrbitCamInputContext>>();
    let updates = query
        .iter(world)
        .filter_map(|(camera, gated, current)| {
            let allowed = gated.context_gate.is_allowed();
            let desired = ContextActivity::<OrbitCamInputContext>::new(allowed);
            if current.is_none_or(|current| **current != allowed) {
                Some((camera, desired))
            } else {
                None
            }
        })
        .collect::<Vec<_>>();
    let gated_cameras = updates.len();

    for (camera, desired) in updates {
        world.entity_mut(camera).insert(desired);
    }

    world
        .resource_mut::<OrbitCamAdapterDiagnostics>()
        .gated_cameras = gated_cameras;
}
