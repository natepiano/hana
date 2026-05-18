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

use bevy::prelude::*;
use bevy_enhanced_input::prelude::Accumulation;
use bevy_enhanced_input::prelude::Action;
use bevy_enhanced_input::prelude::ActionOf;
use bevy_enhanced_input::prelude::ActionSettings;
use bevy_enhanced_input::prelude::Actions;
use bevy_enhanced_input::prelude::Binding;
use bevy_enhanced_input::prelude::BindingOf;
use bevy_enhanced_input::prelude::ContextActivity;
use bevy_enhanced_input::prelude::ContextPriority;
use bevy_enhanced_input::prelude::GamepadDevice;
use bevy_enhanced_input::prelude::InputAction;
use bevy_enhanced_input::prelude::Negate;
use bevy_enhanced_input::prelude::SwizzleAxis;

use super::inject::OrbitCamAdapterFrameSources;
use crate::input::ActionBindingEntry;
use crate::input::CameraInputGamepadSelectionPolicy;
use crate::input::CameraInteractionSources;
use crate::input::CameraSemanticAction;
use crate::input::HeldActionBindingEntry;
use crate::input::HeldCameraAction;
use crate::input::InputBindingDescriptor;
use crate::input::InputBindingTransform;
use crate::input::OrbitCamBindings;
use crate::input::OrbitCamInputContext;
use crate::input::OrbitCamInputContextGated;
use crate::input::OrbitCamOrbitAction;
use crate::input::OrbitCamPanAction;
use crate::input::OrbitCamPreset;
use crate::input::OrbitCamZoomCoarseAction;
use crate::input::OrbitCamZoomSmoothAction;
use crate::input::actions::OrbitCamAdapterOrbitAction;
use crate::input::actions::OrbitCamAdapterPanAction;
use crate::input::actions::OrbitCamAdapterZoomCoarseAction;
use crate::input::actions::OrbitCamAdapterZoomSmoothAction;
use crate::input::actions::OrbitCamOrbitEngagedAction;
use crate::input::actions::OrbitCamPanEngagedAction;
use crate::input::actions::OrbitCamZoomEngagedAction;
use crate::input::modes;
use crate::input::modes::OrbitCamInputInstallationOf;
use crate::orbit_cam::OrbitCam;

#[derive(Component, Clone, Debug, PartialEq)]
pub(super) struct OrbitCamInstalledBindings(pub(super) OrbitCamBindings);

#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct OrbitCamInputActionEntities {
    pub(super) orbit:               Entity,
    pub(super) orbit_engaged:       Entity,
    pub(super) pan:                 Entity,
    pub(super) pan_engaged:         Entity,
    pub(super) zoom_coarse:         Entity,
    pub(super) zoom_smooth:         Entity,
    pub(super) zoom_engaged:        Entity,
    pub(super) adapter_orbit:       Entity,
    pub(super) adapter_pan:         Entity,
    pub(super) adapter_zoom_coarse: Entity,
    pub(super) adapter_zoom_smooth: Entity,
    pub(super) orbit_sources:       CameraInteractionSources,
    pub(super) pan_sources:         CameraInteractionSources,
    pub(super) zoom_coarse_sources: CameraInteractionSources,
    pub(super) zoom_smooth_sources: CameraInteractionSources,
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
        .remove::<OrbitCamAdapterFrameSources>();
}

pub(super) fn install_enhanced_input_entities(world: &mut World) {
    let mut query = world.query_filtered::<(
        Entity,
        Option<&OrbitCamPreset>,
        Option<&OrbitCamBindings>,
    ), With<OrbitCam>>();
    let cameras = query
        .iter(world)
        .filter(|(camera, _, _)| modes::input_installation_has_placeholder(world, *camera))
        .map(|(camera, preset, bindings)| (camera, preset.copied(), bindings.cloned()))
        .collect::<Vec<_>>();

    let mut installed_cameras = 0;
    let mut installed_entities = 0;

    for (camera, preset, bindings) in cameras {
        let Some(bindings) = bindings.or_else(|| preset.unwrap_or_default().to_bindings().ok())
        else {
            warn!("failed to build OrbitCam input bindings for {camera:?}");
            continue;
        };

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
    actions:  OrbitCamInputActionEntities,
    entities: Vec<Entity>,
}

fn spawn_input_installation(
    world: &mut World,
    camera: Entity,
    bindings: &OrbitCamBindings,
) -> SpawnedInputInstallation {
    let orbit = spawn_action::<OrbitCamOrbitAction>(world, camera);
    let orbit_engaged = spawn_action::<OrbitCamOrbitEngagedAction>(world, camera);
    let pan = spawn_action::<OrbitCamPanAction>(world, camera);
    let pan_engaged = spawn_action::<OrbitCamPanEngagedAction>(world, camera);
    let zoom_coarse = spawn_action::<OrbitCamZoomCoarseAction>(world, camera);
    let zoom_smooth = spawn_action::<OrbitCamZoomSmoothAction>(world, camera);
    let zoom_engaged = spawn_action::<OrbitCamZoomEngagedAction>(world, camera);
    let adapter_orbit = spawn_action::<OrbitCamAdapterOrbitAction>(world, camera);
    let adapter_pan = spawn_action::<OrbitCamAdapterPanAction>(world, camera);
    let adapter_zoom_coarse = spawn_action::<OrbitCamAdapterZoomCoarseAction>(world, camera);
    let adapter_zoom_smooth = spawn_action::<OrbitCamAdapterZoomSmoothAction>(world, camera);

    let mut entities = vec![
        orbit,
        orbit_engaged,
        pan,
        pan_engaged,
        zoom_coarse,
        zoom_smooth,
        zoom_engaged,
        adapter_orbit,
        adapter_pan,
        adapter_zoom_coarse,
        adapter_zoom_smooth,
    ];

    spawn_held_bindings(
        world,
        camera,
        orbit,
        orbit_engaged,
        bindings.orbit().entries(),
        &mut entities,
    );
    spawn_held_bindings(
        world,
        camera,
        pan,
        pan_engaged,
        bindings.pan().entries(),
        &mut entities,
    );
    spawn_held_bindings(
        world,
        camera,
        zoom_smooth,
        zoom_engaged,
        bindings.zoom_smooth().entries(),
        &mut entities,
    );
    for entry in bindings.zoom_coarse().entries() {
        entities.extend(spawn_binding(
            world,
            camera,
            zoom_coarse,
            entry.binding_descriptor(),
        ));
    }

    SpawnedInputInstallation {
        actions: OrbitCamInputActionEntities {
            orbit,
            orbit_engaged,
            pan,
            pan_engaged,
            zoom_coarse,
            zoom_smooth,
            zoom_engaged,
            adapter_orbit,
            adapter_pan,
            adapter_zoom_coarse,
            adapter_zoom_smooth,
            orbit_sources: held_sources(bindings.orbit().entries()),
            pan_sources: held_sources(bindings.pan().entries()),
            zoom_coarse_sources: action_sources(bindings.zoom_coarse().entries()),
            zoom_smooth_sources: held_sources(bindings.zoom_smooth().entries()),
        },
        entities,
    }
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
    motion_action: Entity,
    engagement_action: Entity,
    entries: &[HeldActionBindingEntry<A>],
    entities: &mut Vec<Entity>,
) {
    for entry in entries {
        entities.extend(spawn_binding(
            world,
            camera,
            motion_action,
            entry.motion_descriptor(),
        ));
        entities.extend(spawn_binding(
            world,
            camera,
            engagement_action,
            entry.engagement_descriptor(),
        ));
    }
}

fn spawn_binding(
    world: &mut World,
    camera: Entity,
    action: Entity,
    binding_descriptor: &InputBindingDescriptor,
) -> Vec<Entity> {
    let installation = OrbitCamInputInstallationOf(camera);
    binding_descriptor
        .entries_slice()
        .iter()
        .map(|entry| {
            spawn_binding_entry(world, action, installation, entry.binding, entry.transform)
        })
        .collect()
}

fn spawn_binding_entry(
    world: &mut World,
    action: Entity,
    installation: OrbitCamInputInstallationOf,
    binding: Binding,
    transform: InputBindingTransform,
) -> Entity {
    match transform {
        InputBindingTransform::None => spawn_single_binding(world, action, installation, binding),
        InputBindingTransform::Negate => {
            spawn_modified_binding(world, action, installation, binding, Negate::all())
        },
        InputBindingTransform::Swizzle => {
            spawn_swizzled_binding(world, action, installation, binding)
        },
        InputBindingTransform::SwizzleNegate => {
            spawn_swizzled_modified_binding(world, action, installation, binding, Negate::all())
        },
    }
}

fn spawn_single_binding(
    world: &mut World,
    action: Entity,
    installation: OrbitCamInputInstallationOf,
    binding: Binding,
) -> Entity {
    world.spawn((binding, BindingOf(action), installation)).id()
}

fn spawn_modified_binding(
    world: &mut World,
    action: Entity,
    installation: OrbitCamInputInstallationOf,
    binding: Binding,
    modifier: Negate,
) -> Entity {
    world
        .spawn((binding, BindingOf(action), installation, modifier))
        .id()
}

fn spawn_swizzled_binding(
    world: &mut World,
    action: Entity,
    installation: OrbitCamInputInstallationOf,
    binding: Binding,
) -> Entity {
    world
        .spawn((binding, BindingOf(action), installation, SwizzleAxis::YXZ))
        .id()
}

fn spawn_swizzled_modified_binding(
    world: &mut World,
    action: Entity,
    installation: OrbitCamInputInstallationOf,
    binding: Binding,
    modifier: Negate,
) -> Entity {
    world
        .spawn((
            binding,
            BindingOf(action),
            installation,
            SwizzleAxis::YXZ,
            modifier,
        ))
        .id()
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
