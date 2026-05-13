use bevy::ecs::system::SystemParam;
use bevy::input::gestures::PinchGesture;
use bevy::input::mouse::AccumulatedMouseMotion;
use bevy::input::mouse::AccumulatedMouseScroll;
use bevy::input::mouse::MouseScrollUnit;
use bevy::prelude::*;
use bevy_enhanced_input::prelude::Accumulation;
use bevy_enhanced_input::prelude::Action;
use bevy_enhanced_input::prelude::ActionMock;
use bevy_enhanced_input::prelude::ActionOf;
use bevy_enhanced_input::prelude::ActionSettings;
use bevy_enhanced_input::prelude::ActionValue;
use bevy_enhanced_input::prelude::Actions;
use bevy_enhanced_input::prelude::Binding;
use bevy_enhanced_input::prelude::BindingOf;
use bevy_enhanced_input::prelude::ContextActivity;
use bevy_enhanced_input::prelude::ContextPriority;
use bevy_enhanced_input::prelude::GamepadDevice;
use bevy_enhanced_input::prelude::InputAction;
use bevy_enhanced_input::prelude::MockSpan;
use bevy_enhanced_input::prelude::ModKeys;
use bevy_enhanced_input::prelude::Negate;
use bevy_enhanced_input::prelude::SwizzleAxis;
use bevy_enhanced_input::prelude::TriggerState;

use super::ActionBindingEntry;
use super::CameraInputGamepadSelectionPolicy;
use super::CameraInteractionSources;
use super::HeldActionBindingEntry;
use super::HeldCameraAction;
use super::InputBindingDescriptor;
use super::InputBindingTransform;
use super::OrbitCamBindings;
use super::OrbitCamButtonDragZoomAxis;
use super::OrbitCamInput;
use super::OrbitCamInputContext;
use super::OrbitCamInputContextGated;
use super::OrbitCamOrbitAction;
use super::OrbitCamPanAction;
use super::OrbitCamPreset;
use super::OrbitCamTouchBinding;
use super::OrbitCamTrackpadScroll;
use super::OrbitCamZoomCoarseAction;
use super::OrbitCamZoomSmoothAction;
use super::ResolvedOrbitCamInputRoute;
use super::ZoomDirection;
use super::actions::OrbitCamAdapterOrbitAction;
use super::actions::OrbitCamAdapterPanAction;
use super::actions::OrbitCamAdapterZoomCoarseAction;
use super::actions::OrbitCamAdapterZoomSmoothAction;
use super::actions::OrbitCamOrbitEngagedAction;
use super::actions::OrbitCamPanEngagedAction;
use super::actions::OrbitCamZoomEngagedAction;
use super::mod_keys_pressed;
use super::modes;
use super::modes::OrbitCamInputInstallationOf;
use crate::constants::BUTTON_ZOOM_SCALE;
use crate::constants::PINCH_GESTURE_AMPLIFICATION;
use crate::constants::PIXEL_SCROLL_SCALE;
use crate::constants::TOUCH_PINCH_SCALE;
use crate::orbit_cam::OrbitCam;
use crate::system_sets::OrbitCamInputInternalSet;
use crate::touch::TouchGestures;
use crate::touch::TouchTracker;

#[derive(Component, Clone, Debug, PartialEq)]
struct OrbitCamInstalledBindings(OrbitCamBindings);

#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
struct OrbitCamInputActionEntities {
    orbit:               Entity,
    orbit_engaged:       Entity,
    pan:                 Entity,
    pan_engaged:         Entity,
    zoom_coarse:         Entity,
    zoom_smooth:         Entity,
    zoom_engaged:        Entity,
    adapter_orbit:       Entity,
    adapter_pan:         Entity,
    adapter_zoom_coarse: Entity,
    adapter_zoom_smooth: Entity,
    orbit_sources:       CameraInteractionSources,
    pan_sources:         CameraInteractionSources,
    zoom_coarse_sources: CameraInteractionSources,
    zoom_smooth_sources: CameraInteractionSources,
}

#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq)]
struct OrbitCamAdapterFrameSources {
    orbit:       CameraInteractionSources,
    pan:         CameraInteractionSources,
    zoom_coarse: CameraInteractionSources,
    zoom_smooth: CameraInteractionSources,
}

#[derive(Resource, Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct OrbitCamAdapterDiagnostics {
    pub(crate) installed_cameras:  usize,
    pub(crate) installed_entities: usize,
    pub(crate) gated_cameras:      usize,
}

pub(crate) struct OrbitCamInputAdapterPlugin;

impl Plugin for OrbitCamInputAdapterPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<OrbitCamAdapterDiagnostics>()
            .add_systems(
                PreUpdate,
                (
                    clear_replaced_or_manual_installations,
                    install_enhanced_input_entities,
                    apply_context_gating,
                )
                    .chain()
                    .in_set(OrbitCamInputInternalSet::Installation),
            )
            .add_systems(
                PreUpdate,
                inject_adapter_actions.in_set(OrbitCamInputInternalSet::AdapterInjection),
            )
            .add_systems(
                PreUpdate,
                resolve_actions_into_orbit_cam_input
                    .in_set(OrbitCamInputInternalSet::ActionResolution),
            );
    }
}

fn clear_replaced_or_manual_installations(world: &mut World) {
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

fn install_enhanced_input_entities(world: &mut World) {
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
    A: super::CameraSemanticAction,
{
    entries
        .iter()
        .fold(CameraInteractionSources::NONE, |sources, entry| {
            sources.union(entry.sources())
        })
}

fn apply_context_gating(world: &mut World) {
    let mut query = world.query_filtered::<(
        Entity,
        &OrbitCamInputContextGated,
        Option<&ContextActivity<OrbitCamInputContext>>,
    ), With<OrbitCamInputContext>>();
    let updates = query
        .iter(world)
        .filter_map(|(camera, gated, current)| {
            let desired = ContextActivity::<OrbitCamInputContext>::new(gated.allowed);
            if current.is_none_or(|current| **current != gated.allowed) {
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

fn inject_adapter_actions(
    route: Res<ResolvedOrbitCamInputRoute>,
    scroll: Option<Res<AccumulatedMouseScroll>>,
    motion: Option<Res<AccumulatedMouseMotion>>,
    keyboard: Option<Res<ButtonInput<KeyCode>>>,
    mouse_buttons: Option<Res<ButtonInput<MouseButton>>>,
    mut pinch_events: MessageReader<PinchGesture>,
    touch_tracker: Option<Res<TouchTracker>>,
    mut cameras: Query<(
        Entity,
        &OrbitCamInstalledBindings,
        &OrbitCamInputActionEntities,
        Option<&OrbitCamInputContextGated>,
        &mut OrbitCamAdapterFrameSources,
    )>,
    mut mocks: Query<&mut ActionMock>,
    #[cfg(test)] touch_override: Option<Res<OrbitCamTouchAdapterOverride>>,
) {
    let scroll = scroll.as_deref().copied().unwrap_or_default();
    let motion = motion
        .as_deref()
        .map(|motion| motion.delta)
        .unwrap_or_default();
    let pinch = pinch_events.read().map(|event| event.0).sum::<f32>();
    let touch_gestures = {
        #[cfg(test)]
        if let Some(override_gestures) = touch_override {
            override_gestures.0.clone()
        } else if let Some(touch_tracker) = touch_tracker {
            touch_tracker.get_touch_gestures()
        } else {
            TouchGestures::None
        }
        #[cfg(not(test))]
        if let Some(touch_tracker) = touch_tracker {
            touch_tracker.get_touch_gestures()
        } else {
            TouchGestures::None
        }
    };

    for (camera, bindings, actions, gated, mut frame_sources) in &mut cameras {
        *frame_sources = OrbitCamAdapterFrameSources::default();
        if route.routed_camera() != Some(camera)
            || route.metrics_for(camera).is_none()
            || route
                .blockers_for(camera)
                .is_some_and(super::routing::OrbitCamInputBlockers::is_blocked)
            || gated.is_some_and(|gated| !gated.allowed)
        {
            clear_adapter_mocks(actions, &mut mocks);
            continue;
        }

        let contributions = adapter_contributions(
            &bindings.0,
            scroll,
            motion,
            pinch,
            &touch_gestures,
            keyboard.as_deref(),
            mouse_buttons.as_deref(),
        );
        *frame_sources = contributions.sources;
        mock_adapter_actions(actions, contributions, &mut mocks);
    }
}

fn clear_adapter_mocks(actions: &OrbitCamInputActionEntities, mocks: &mut Query<&mut ActionMock>) {
    set_mock(mocks, actions.adapter_orbit, TriggerState::None, Vec2::ZERO);
    set_mock(mocks, actions.adapter_pan, TriggerState::None, Vec2::ZERO);
    set_mock(mocks, actions.adapter_zoom_coarse, TriggerState::None, 0.0);
    set_mock(mocks, actions.adapter_zoom_smooth, TriggerState::None, 0.0);
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
struct AdapterContributions {
    orbit:       Vec2,
    pan:         Vec2,
    zoom_coarse: f32,
    zoom_smooth: f32,
    sources:     OrbitCamAdapterFrameSources,
}

fn adapter_contributions(
    bindings: &OrbitCamBindings,
    scroll: AccumulatedMouseScroll,
    mouse_motion: Vec2,
    pinch: f32,
    touch_gestures: &TouchGestures,
    keyboard: Option<&ButtonInput<KeyCode>>,
    mouse_buttons: Option<&ButtonInput<MouseButton>>,
) -> AdapterContributions {
    let mut contributions = AdapterContributions::default();
    apply_mouse_wheel_zoom_contribution(bindings, scroll, &mut contributions);
    apply_trackpad_scroll_contribution(bindings, scroll, keyboard, &mut contributions);
    apply_pinch_contribution(bindings, pinch, keyboard, mouse_buttons, &mut contributions);
    apply_touch_contribution(bindings, touch_gestures, &mut contributions);
    apply_button_drag_zoom_contribution(bindings, mouse_motion, mouse_buttons, &mut contributions);
    contributions
}

fn apply_mouse_wheel_zoom_contribution(
    bindings: &OrbitCamBindings,
    scroll: AccumulatedMouseScroll,
    contributions: &mut AdapterContributions,
) {
    let Some(mouse_wheel_zoom) = bindings.mouse_wheel_zoom() else {
        return;
    };
    if scroll.delta == Vec2::ZERO || scroll.unit != MouseScrollUnit::Line {
        return;
    }

    contributions.zoom_coarse += zoom_signed(scroll.delta.y, bindings, mouse_wheel_zoom.inverted);
    contributions.sources.zoom_coarse = contributions
        .sources
        .zoom_coarse
        .union(CameraInteractionSources::WHEEL);
}

fn apply_trackpad_scroll_contribution(
    bindings: &OrbitCamBindings,
    scroll: AccumulatedMouseScroll,
    keyboard: Option<&ButtonInput<KeyCode>>,
    contributions: &mut AdapterContributions,
) {
    if scroll.delta == Vec2::ZERO || scroll.unit != MouseScrollUnit::Pixel {
        return;
    }

    match selected_trackpad_binding(bindings, keyboard) {
        Some(TrackpadScrollTarget::Orbit) => {
            contributions.orbit += scroll.delta;
            contributions.sources.orbit = contributions
                .sources
                .orbit
                .union(CameraInteractionSources::SMOOTH_SCROLL);
        },
        Some(TrackpadScrollTarget::Pan) => {
            contributions.pan += scroll.delta;
            contributions.sources.pan = contributions
                .sources
                .pan
                .union(CameraInteractionSources::SMOOTH_SCROLL);
        },
        Some(TrackpadScrollTarget::Zoom) => {
            contributions.zoom_smooth +=
                zoom_signed(scroll.delta.y * PIXEL_SCROLL_SCALE, bindings, false);
            contributions.sources.zoom_smooth = contributions
                .sources
                .zoom_smooth
                .union(CameraInteractionSources::SMOOTH_SCROLL);
        },
        None => {},
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum TrackpadScrollTarget {
    Orbit,
    Pan,
    Zoom,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct TrackpadScrollCandidate {
    target:   TrackpadScrollTarget,
    mod_keys: ModKeys,
}

fn selected_trackpad_binding(
    bindings: &OrbitCamBindings,
    keyboard: Option<&ButtonInput<KeyCode>>,
) -> Option<TrackpadScrollTarget> {
    let candidates = bindings
        .trackpad_orbit()
        .iter()
        .map(|binding| trackpad_candidate(TrackpadScrollTarget::Orbit, *binding))
        .chain(
            bindings
                .trackpad_pan()
                .iter()
                .map(|binding| trackpad_candidate(TrackpadScrollTarget::Pan, *binding)),
        )
        .chain(
            bindings
                .trackpad_zoom()
                .iter()
                .map(|binding| trackpad_candidate(TrackpadScrollTarget::Zoom, *binding)),
        );

    candidates
        .filter(|candidate| trackpad_mod_keys_pressed(keyboard, candidate.mod_keys))
        .max_by_key(|candidate| {
            (
                mod_key_count(candidate.mod_keys),
                trackpad_target_priority(candidate.target),
            )
        })
        .map(|candidate| candidate.target)
}

const fn trackpad_candidate(
    target: TrackpadScrollTarget,
    binding: OrbitCamTrackpadScroll,
) -> TrackpadScrollCandidate {
    TrackpadScrollCandidate {
        target,
        mod_keys: binding.mod_keys,
    }
}

fn trackpad_mod_keys_pressed(keyboard: Option<&ButtonInput<KeyCode>>, mod_keys: ModKeys) -> bool {
    if mod_keys.is_empty() {
        return true;
    }
    keyboard.is_some_and(|keyboard| mod_keys_pressed(keyboard, mod_keys))
}

fn mod_key_count(mod_keys: ModKeys) -> usize { mod_keys.iter_names().count() }

const fn trackpad_target_priority(target: TrackpadScrollTarget) -> usize {
    match target {
        TrackpadScrollTarget::Orbit => 0,
        TrackpadScrollTarget::Pan => 1,
        TrackpadScrollTarget::Zoom => 2,
    }
}

fn apply_pinch_contribution(
    bindings: &OrbitCamBindings,
    pinch: f32,
    keyboard: Option<&ButtonInput<KeyCode>>,
    mouse_buttons: Option<&ButtonInput<MouseButton>>,
    contributions: &mut AdapterContributions,
) {
    if !bindings.pinch_zoom() || pinch == 0.0 || pinch_suppressed(bindings, keyboard, mouse_buttons)
    {
        return;
    }

    contributions.zoom_smooth += zoom_signed(pinch * PINCH_GESTURE_AMPLIFICATION, bindings, false);
    contributions.sources.zoom_smooth = contributions
        .sources
        .zoom_smooth
        .union(CameraInteractionSources::PINCH);
}

fn pinch_suppressed(
    bindings: &OrbitCamBindings,
    keyboard: Option<&ButtonInput<KeyCode>>,
    mouse_buttons: Option<&ButtonInput<MouseButton>>,
) -> bool {
    trackpad_modifier_active(bindings, keyboard)
        || bindings.orbit().entries().iter().any(|entry| {
            entry
                .engagement_descriptor()
                .is_active(keyboard, mouse_buttons)
        })
        || bindings.pan().entries().iter().any(|entry| {
            entry
                .engagement_descriptor()
                .is_active(keyboard, mouse_buttons)
        })
        || bindings.zoom_smooth().entries().iter().any(|entry| {
            entry
                .engagement_descriptor()
                .is_active(keyboard, mouse_buttons)
        })
}

fn trackpad_modifier_active(
    bindings: &OrbitCamBindings,
    keyboard: Option<&ButtonInput<KeyCode>>,
) -> bool {
    let Some(keyboard) = keyboard else {
        return false;
    };

    bindings
        .trackpad_orbit()
        .iter()
        .chain(bindings.trackpad_pan())
        .chain(bindings.trackpad_zoom())
        .any(|binding| !binding.mod_keys.is_empty() && mod_keys_pressed(keyboard, binding.mod_keys))
}

fn apply_touch_contribution(
    bindings: &OrbitCamBindings,
    touch_gestures: &TouchGestures,
    contributions: &mut AdapterContributions,
) {
    let Some(touch) = bindings.touch() else {
        return;
    };

    let (orbit, pan, zoom) = match (touch, touch_gestures) {
        (OrbitCamTouchBinding::OneFingerOrbit, TouchGestures::OneFinger(gesture)) => {
            (gesture.motion, Vec2::ZERO, 0.0)
        },
        (OrbitCamTouchBinding::OneFingerOrbit, TouchGestures::TwoFinger(gesture)) => (
            Vec2::ZERO,
            gesture.motion,
            gesture.pinch * TOUCH_PINCH_SCALE,
        ),
        (OrbitCamTouchBinding::TwoFingerOrbit, TouchGestures::OneFinger(gesture)) => {
            (Vec2::ZERO, gesture.motion, 0.0)
        },
        (OrbitCamTouchBinding::TwoFingerOrbit, TouchGestures::TwoFinger(gesture)) => (
            gesture.motion,
            Vec2::ZERO,
            gesture.pinch * TOUCH_PINCH_SCALE,
        ),
        (_, TouchGestures::None) => (Vec2::ZERO, Vec2::ZERO, 0.0),
    };

    if orbit != Vec2::ZERO {
        contributions.orbit += orbit;
        contributions.sources.orbit = contributions
            .sources
            .orbit
            .union(CameraInteractionSources::TOUCH);
    }
    if pan != Vec2::ZERO {
        contributions.pan += pan;
        contributions.sources.pan = contributions
            .sources
            .pan
            .union(CameraInteractionSources::TOUCH);
    }
    if zoom != 0.0 {
        contributions.zoom_smooth += zoom_signed(zoom, bindings, false);
        contributions.sources.zoom_smooth = contributions
            .sources
            .zoom_smooth
            .union(CameraInteractionSources::TOUCH);
    }
}

fn apply_button_drag_zoom_contribution(
    bindings: &OrbitCamBindings,
    mouse_motion: Vec2,
    mouse_buttons: Option<&ButtonInput<MouseButton>>,
    contributions: &mut AdapterContributions,
) {
    let Some(button_drag_zoom) = bindings.button_drag_zoom() else {
        return;
    };
    if mouse_motion == Vec2::ZERO
        || mouse_buttons.is_none_or(|buttons| !buttons.pressed(button_drag_zoom.button))
    {
        return;
    }

    let delta = match button_drag_zoom.axis {
        OrbitCamButtonDragZoomAxis::X => mouse_motion.x,
        OrbitCamButtonDragZoomAxis::Y => -mouse_motion.y,
        OrbitCamButtonDragZoomAxis::XY => mouse_motion.x - mouse_motion.y,
    };
    contributions.zoom_smooth += zoom_signed(delta * BUTTON_ZOOM_SCALE, bindings, false);
    contributions.sources.zoom_smooth = contributions
        .sources
        .zoom_smooth
        .union(CameraInteractionSources::MOUSE);
}

fn zoom_signed(value: f32, bindings: &OrbitCamBindings, inverted: bool) -> f32 {
    let wheel_sign = if inverted { -1.0 } else { 1.0 };
    let zoom_sign = match bindings.zoom_direction() {
        ZoomDirection::Normal => 1.0,
        ZoomDirection::Reversed => -1.0,
    };
    value * wheel_sign * zoom_sign
}

fn mock_adapter_actions(
    actions: &OrbitCamInputActionEntities,
    contributions: AdapterContributions,
    mocks: &mut Query<&mut ActionMock>,
) {
    set_mock(
        mocks,
        actions.adapter_orbit,
        state_for_vec2(contributions.orbit),
        contributions.orbit,
    );
    set_mock(
        mocks,
        actions.adapter_pan,
        state_for_vec2(contributions.pan),
        contributions.pan,
    );
    set_mock(
        mocks,
        actions.adapter_zoom_coarse,
        state_for_f32(contributions.zoom_coarse),
        contributions.zoom_coarse,
    );
    set_mock(
        mocks,
        actions.adapter_zoom_smooth,
        state_for_f32(contributions.zoom_smooth),
        contributions.zoom_smooth,
    );
}

fn set_mock(
    mocks: &mut Query<&mut ActionMock>,
    action: Entity,
    state: TriggerState,
    value: impl Into<ActionValue>,
) {
    if let Ok(mut mock) = mocks.get_mut(action) {
        *mock = ActionMock::new(state, value, MockSpan::once());
    }
}

const fn state_for_vec2(value: Vec2) -> TriggerState {
    if value.x == 0.0 && value.y == 0.0 {
        TriggerState::None
    } else {
        TriggerState::Fired
    }
}

const fn state_for_f32(value: f32) -> TriggerState {
    if value == 0.0 {
        TriggerState::None
    } else {
        TriggerState::Fired
    }
}

#[allow(
    clippy::too_many_lines,
    reason = "the resolver keeps related enhanced-input query reads in one scheduling system"
)]
fn resolve_actions_into_orbit_cam_input(
    route: Res<ResolvedOrbitCamInputRoute>,
    mut cameras: Query<
        (
            Entity,
            &OrbitCamInstalledBindings,
            &OrbitCamInputActionEntities,
            &OrbitCamAdapterFrameSources,
            Option<&OrbitCamInputContextGated>,
            &mut OrbitCamInput,
        ),
        Without<super::OrbitCamManual>,
    >,
    vec2_actions: Vec2ActionQueries,
    f32_actions: F32ActionQueries,
    bool_actions: BoolActionQueries,
    states: Query<&TriggerState>,
    keyboard: Option<Res<ButtonInput<KeyCode>>>,
    mouse_buttons: Option<Res<ButtonInput<MouseButton>>>,
) {
    for (camera, bindings, actions, frame_sources, gated, mut input) in &mut cameras {
        input.clear();
        if route.routed_camera() != Some(camera)
            || route.metrics_for(camera).is_none()
            || route
                .blockers_for(camera)
                .is_some_and(super::routing::OrbitCamInputBlockers::is_blocked)
            || gated.is_some_and(|gated| !gated.allowed)
        {
            continue;
        }

        let orbit_engaged = bool_action_active(actions.orbit_engaged, &bool_actions.orbit, &states);
        let pan_engaged = bool_action_active(actions.pan_engaged, &bool_actions.pan, &states);
        let zoom_engaged = bool_action_active(actions.zoom_engaged, &bool_actions.zoom, &states);
        let pan_overrides_orbit =
            pan_overrides_orbit(&bindings.0, keyboard.as_deref(), mouse_buttons.as_deref());
        let orbit_sources = held_sources_for_state(
            orbit_engaged,
            bindings.0.orbit().entries(),
            actions.orbit_sources,
            keyboard.as_deref(),
            mouse_buttons.as_deref(),
        );
        let pan_sources = held_sources_for_state(
            pan_engaged,
            bindings.0.pan().entries(),
            actions.pan_sources,
            keyboard.as_deref(),
            mouse_buttons.as_deref(),
        );
        let zoom_smooth_sources = held_sources_for_state(
            zoom_engaged,
            bindings.0.zoom_smooth().entries(),
            actions.zoom_smooth_sources,
            keyboard.as_deref(),
            mouse_buttons.as_deref(),
        );

        if orbit_engaged && !pan_overrides_orbit {
            input.orbit_pixels_with_sources(
                action_value(actions.orbit, &vec2_actions.orbit),
                orbit_sources,
            );
        }
        if pan_engaged {
            input
                .pan_pixels_with_sources(action_value(actions.pan, &vec2_actions.pan), pan_sources);
        }
        if zoom_engaged {
            input.zoom_smooth_with_sources(
                action_value(actions.zoom_smooth, &f32_actions.zoom_smooth),
                zoom_smooth_sources,
            );
        }
        if action_state_active(actions.zoom_coarse, &states) {
            input.zoom_coarse_with_sources(
                action_value(actions.zoom_coarse, &f32_actions.zoom_coarse),
                actions.zoom_coarse_sources,
            );
        }

        if action_state_active(actions.adapter_orbit, &states) {
            input.orbit_pixels_with_sources(
                action_value(actions.adapter_orbit, &vec2_actions.adapter_orbit),
                frame_sources.orbit,
            );
        }
        if action_state_active(actions.adapter_pan, &states) {
            input.pan_pixels_with_sources(
                action_value(actions.adapter_pan, &vec2_actions.adapter_pan),
                frame_sources.pan,
            );
        }
        if action_state_active(actions.adapter_zoom_coarse, &states) {
            input.zoom_coarse_with_sources(
                action_value(
                    actions.adapter_zoom_coarse,
                    &f32_actions.adapter_zoom_coarse,
                ),
                frame_sources.zoom_coarse,
            );
        }
        if action_state_active(actions.adapter_zoom_smooth, &states) {
            input.zoom_smooth_with_sources(
                action_value(
                    actions.adapter_zoom_smooth,
                    &f32_actions.adapter_zoom_smooth,
                ),
                frame_sources.zoom_smooth,
            );
        }
    }
}

#[derive(SystemParam)]
struct Vec2ActionQueries<'w, 's> {
    orbit:         Query<'w, 's, &'static Action<OrbitCamOrbitAction>>,
    pan:           Query<'w, 's, &'static Action<OrbitCamPanAction>>,
    adapter_orbit: Query<'w, 's, &'static Action<OrbitCamAdapterOrbitAction>>,
    adapter_pan:   Query<'w, 's, &'static Action<OrbitCamAdapterPanAction>>,
}

#[derive(SystemParam)]
struct F32ActionQueries<'w, 's> {
    zoom_coarse:         Query<'w, 's, &'static Action<OrbitCamZoomCoarseAction>>,
    zoom_smooth:         Query<'w, 's, &'static Action<OrbitCamZoomSmoothAction>>,
    adapter_zoom_coarse: Query<'w, 's, &'static Action<OrbitCamAdapterZoomCoarseAction>>,
    adapter_zoom_smooth: Query<'w, 's, &'static Action<OrbitCamAdapterZoomSmoothAction>>,
}

#[derive(SystemParam)]
struct BoolActionQueries<'w, 's> {
    orbit: Query<'w, 's, &'static Action<OrbitCamOrbitEngagedAction>>,
    pan:   Query<'w, 's, &'static Action<OrbitCamPanEngagedAction>>,
    zoom:  Query<'w, 's, &'static Action<OrbitCamZoomEngagedAction>>,
}

fn held_sources_for_state<A: HeldCameraAction>(
    engaged: bool,
    entries: &[HeldActionBindingEntry<A>],
    fallback: CameraInteractionSources,
    keyboard: Option<&ButtonInput<KeyCode>>,
    mouse_buttons: Option<&ButtonInput<MouseButton>>,
) -> CameraInteractionSources {
    if !engaged {
        return CameraInteractionSources::NONE;
    }

    let active_sources = entries
        .iter()
        .filter(|entry| {
            entry
                .engagement_descriptor()
                .is_active(keyboard, mouse_buttons)
        })
        .fold(CameraInteractionSources::NONE, |sources, entry| {
            sources.union(entry.sources())
        });
    if active_sources.is_empty() {
        fallback
    } else {
        active_sources
    }
}

fn pan_overrides_orbit(
    bindings: &OrbitCamBindings,
    keyboard: Option<&ButtonInput<KeyCode>>,
    mouse_buttons: Option<&ButtonInput<MouseButton>>,
) -> bool {
    bindings.pan().entries().iter().any(|pan| {
        let Some((pan_button, pan_mod_keys)) =
            pan.engagement_descriptor().mouse_button_engagement()
        else {
            return false;
        };
        if !pan
            .engagement_descriptor()
            .is_active(keyboard, mouse_buttons)
        {
            return false;
        }
        bindings.orbit().entries().iter().any(|orbit| {
            let Some((orbit_button, orbit_mod_keys)) =
                orbit.engagement_descriptor().mouse_button_engagement()
            else {
                return false;
            };
            pan_button == orbit_button
                && mod_key_count(pan_mod_keys) > mod_key_count(orbit_mod_keys)
                && orbit
                    .engagement_descriptor()
                    .is_active(keyboard, mouse_buttons)
        })
    })
}

fn bool_action_active<A: InputAction<Output = bool>>(
    action: Entity,
    actions: &Query<&Action<A>>,
    states: &Query<&TriggerState>,
) -> bool {
    action_state_active(action, states) && actions.get(action).is_ok_and(|action| **action)
}

fn action_state_active(action: Entity, states: &Query<&TriggerState>) -> bool {
    states
        .get(action)
        .is_ok_and(|state| matches!(*state, TriggerState::Ongoing | TriggerState::Fired))
}

fn action_value<A: InputAction>(action: Entity, actions: &Query<&Action<A>>) -> A::Output {
    actions
        .get(action)
        .map(|action| **action)
        .unwrap_or_default()
}

#[cfg(test)]
#[derive(Resource, Clone, Debug)]
struct OrbitCamTouchAdapterOverride(TouchGestures);

#[cfg(test)]
mod tests {
    use bevy::camera::RenderTarget;
    use bevy::input::gamepad::Gamepad;
    use bevy::input::mouse::AccumulatedMouseMotion;
    use bevy::input::mouse::AccumulatedMouseScroll;
    use bevy::input::mouse::MouseScrollUnit;
    use bevy::prelude::*;
    use bevy::window::WindowRef;

    use super::*;
    use crate::enhanced_input::LagrangeEnhancedInputPlugin;
    use crate::input::CameraInputDisabled;
    use crate::input::CameraInputRoutingConfig;
    use crate::input::OrbitCamHeldBinding;
    use crate::input::OrbitCamInputBinding;
    use crate::input::OrbitCamManual;
    use crate::input::OrbitCamPinchZoom;
    use crate::input::OrbitDelta;
    use crate::input::modes::OrbitCamInputModesPlugin;
    use crate::input::routing::OrbitCamRoutingPlugin;
    use crate::system_sets::LagrangeSystemSetsPlugin;
    use crate::touch::OneFingerGestures;
    use crate::touch::TouchGestures;

    fn test_app() -> App {
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

    fn spawn_camera(world: &mut World, components: impl Bundle) -> Entity {
        world
            .spawn((
                OrbitCam::default(),
                OrbitCamInput::default(),
                Camera::default(),
                RenderTarget::Window(WindowRef::Primary),
                components,
            ))
            .id()
    }

    fn route_to(app: &mut App, camera: Entity) {
        app.insert_resource(CameraInputRoutingConfig::explicit(camera));
    }

    type TestResult = Result<(), &'static str>;

    fn camera_input(app: &App, camera: Entity) -> Result<&OrbitCamInput, &'static str> {
        app.world()
            .get::<OrbitCamInput>(camera)
            .ok_or("camera should have OrbitCamInput")
    }

    fn assert_f32_close(actual: f32, expected: f32) {
        assert!((actual - expected).abs() <= f32::EPSILON);
    }

    #[test]
    fn installer_replaces_placeholder_with_action_entities() {
        let mut app = test_app();
        let camera = spawn_camera(app.world_mut(), OrbitCamPreset::SimpleMouse);
        route_to(&mut app, camera);

        app.update();

        assert!(app.world().get::<OrbitCamInputContext>(camera).is_some());
        assert!(
            app.world()
                .get::<OrbitCamInputActionEntities>(camera)
                .is_some()
        );
        assert!(modes::installed_input_entities(app.world(), camera).len() > 1);
        assert!(!modes::input_installation_has_placeholder(
            app.world(),
            camera
        ));
    }

    #[test]
    fn mouse_drag_action_resolves_to_orbit_input() -> TestResult {
        let mut app = test_app();
        let camera = spawn_camera(app.world_mut(), OrbitCamPreset::SimpleMouse);
        route_to(&mut app, camera);
        app.world_mut()
            .resource_mut::<ButtonInput<MouseButton>>()
            .press(MouseButton::Left);
        app.world_mut()
            .resource_mut::<AccumulatedMouseMotion>()
            .delta = Vec2::new(5.0, -2.0);

        app.update();

        let input = camera_input(&app, camera)?;
        assert_eq!(input.orbit(), OrbitDelta::from(Vec2::new(5.0, -2.0)));
        assert!(input.has_orbit());
        assert!(input.sources().contains(CameraInteractionSources::MOUSE));
        Ok(())
    }

    #[test]
    fn blender_like_shift_middle_mouse_resolves_to_pan_only() -> TestResult {
        let mut app = test_app();
        let camera = spawn_camera(app.world_mut(), OrbitCamPreset::BlenderLike);
        route_to(&mut app, camera);
        app.world_mut()
            .resource_mut::<ButtonInput<MouseButton>>()
            .press(MouseButton::Middle);
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .press(KeyCode::ShiftLeft);
        app.world_mut()
            .resource_mut::<AccumulatedMouseMotion>()
            .delta = Vec2::new(5.0, -2.0);

        app.update();

        let input = camera_input(&app, camera)?;
        assert!(!input.has_orbit());
        assert_eq!(input.pan().pixels(), Vec2::new(5.0, -2.0));
        assert!(input.sources().contains(CameraInteractionSources::MOUSE));
        Ok(())
    }

    #[test]
    fn wheel_line_adapter_resolves_to_coarse_zoom() -> TestResult {
        let mut app = test_app();
        let camera = spawn_camera(app.world_mut(), OrbitCamPreset::SimpleMouse);
        route_to(&mut app, camera);
        *app.world_mut().resource_mut::<AccumulatedMouseScroll>() = AccumulatedMouseScroll {
            unit:  MouseScrollUnit::Line,
            delta: Vec2::new(0.0, 3.0),
        };

        app.update();

        let input = camera_input(&app, camera)?;
        assert_f32_close(input.zoom_coarse().amount(), 3.0);
        assert!(input.has_zoom());
        assert!(input.sources().contains(CameraInteractionSources::WHEEL));
        Ok(())
    }

    #[test]
    fn blender_like_trackpad_shift_resolves_to_pan_only() -> TestResult {
        let mut app = test_app();
        let camera = spawn_camera(app.world_mut(), OrbitCamPreset::BlenderLike);
        route_to(&mut app, camera);
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .press(KeyCode::ShiftLeft);
        *app.world_mut().resource_mut::<AccumulatedMouseScroll>() = AccumulatedMouseScroll {
            unit:  MouseScrollUnit::Pixel,
            delta: Vec2::new(4.0, 6.0),
        };

        app.update();

        let input = camera_input(&app, camera)?;
        assert!(!input.has_orbit());
        assert_eq!(input.pan().pixels(), Vec2::new(4.0, 6.0));
        assert!(
            input
                .sources()
                .contains(CameraInteractionSources::SMOOTH_SCROLL)
        );
        Ok(())
    }

    #[test]
    fn blender_like_trackpad_control_resolves_to_zoom_only() -> TestResult {
        let mut app = test_app();
        let camera = spawn_camera(app.world_mut(), OrbitCamPreset::BlenderLike);
        route_to(&mut app, camera);
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .press(KeyCode::ControlLeft);
        *app.world_mut().resource_mut::<AccumulatedMouseScroll>() = AccumulatedMouseScroll {
            unit:  MouseScrollUnit::Pixel,
            delta: Vec2::new(4.0, 6.0),
        };

        app.update();

        let input = camera_input(&app, camera)?;
        assert!(!input.has_orbit());
        assert!(!input.has_pan());
        assert_f32_close(input.zoom_smooth().amount(), 6.0 * PIXEL_SCROLL_SCALE);
        assert!(
            input
                .sources()
                .contains(CameraInteractionSources::SMOOTH_SCROLL)
        );
        Ok(())
    }

    #[test]
    fn pixel_scroll_adapter_resolves_to_smooth_zoom() -> TestResult {
        let mut app = test_app();
        let bindings = OrbitCamBindings::builder()
            .zoom(OrbitCamTrackpadScroll::default())
            .build()
            .map_err(|_| "bindings should validate")?;
        let camera = spawn_camera(app.world_mut(), bindings);
        route_to(&mut app, camera);
        *app.world_mut().resource_mut::<AccumulatedMouseScroll>() = AccumulatedMouseScroll {
            unit:  MouseScrollUnit::Pixel,
            delta: Vec2::new(0.0, 20.0),
        };

        app.update();

        let input = camera_input(&app, camera)?;
        assert_f32_close(input.zoom_smooth().amount(), 20.0 * PIXEL_SCROLL_SCALE);
        assert!(
            input
                .sources()
                .contains(CameraInteractionSources::SMOOTH_SCROLL)
        );
        Ok(())
    }

    #[test]
    fn pinch_adapter_resolves_to_smooth_zoom() -> TestResult {
        let mut app = test_app();
        let camera = spawn_camera(app.world_mut(), OrbitCamPreset::SimpleMouse);
        route_to(&mut app, camera);
        app.world_mut().write_message(PinchGesture(2.0));

        app.update();

        let input = camera_input(&app, camera)?;
        assert_f32_close(
            input.zoom_smooth().amount(),
            2.0 * PINCH_GESTURE_AMPLIFICATION,
        );
        assert!(input.sources().contains(CameraInteractionSources::PINCH));
        Ok(())
    }

    #[test]
    fn pinch_adapter_is_suppressed_by_routed_held_action() -> TestResult {
        let mut app = test_app();
        let camera = spawn_camera(app.world_mut(), OrbitCamPreset::SimpleMouse);
        route_to(&mut app, camera);
        app.world_mut()
            .resource_mut::<ButtonInput<MouseButton>>()
            .press(MouseButton::Left);
        app.world_mut().write_message(PinchGesture(2.0));

        app.update();

        let input = camera_input(&app, camera)?;
        assert_f32_close(input.zoom_smooth().amount(), 0.0);
        assert!(!input.sources().contains(CameraInteractionSources::PINCH));
        Ok(())
    }

    #[test]
    fn blender_like_shift_modifier_suppresses_pinch() -> TestResult {
        let mut app = test_app();
        let camera = spawn_camera(app.world_mut(), OrbitCamPreset::BlenderLike);
        route_to(&mut app, camera);
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .press(KeyCode::ShiftLeft);
        app.world_mut().write_message(PinchGesture(2.0));

        app.update();

        let input = camera_input(&app, camera)?;
        assert_f32_close(input.zoom_smooth().amount(), 0.0);
        assert!(!input.sources().contains(CameraInteractionSources::PINCH));
        Ok(())
    }

    #[test]
    fn blender_like_control_modifier_suppresses_pinch() -> TestResult {
        let mut app = test_app();
        let camera = spawn_camera(app.world_mut(), OrbitCamPreset::BlenderLike);
        route_to(&mut app, camera);
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .press(KeyCode::ControlLeft);
        app.world_mut().write_message(PinchGesture(2.0));

        app.update();

        let input = camera_input(&app, camera)?;
        assert_f32_close(input.zoom_smooth().amount(), 0.0);
        assert!(!input.sources().contains(CameraInteractionSources::PINCH));
        Ok(())
    }

    #[test]
    fn non_routed_held_action_does_not_suppress_routed_pinch() -> TestResult {
        let mut app = test_app();
        let bindings = OrbitCamBindings::builder()
            .zoom(OrbitCamPinchZoom)
            .build()
            .map_err(|_| "bindings should validate")?;
        let routed = spawn_camera(app.world_mut(), bindings);
        let _non_routed = spawn_camera(app.world_mut(), OrbitCamPreset::SimpleMouse);
        route_to(&mut app, routed);
        app.world_mut()
            .resource_mut::<ButtonInput<MouseButton>>()
            .press(MouseButton::Left);
        app.world_mut().write_message(PinchGesture(2.0));

        app.update();

        let input = camera_input(&app, routed)?;
        assert_f32_close(
            input.zoom_smooth().amount(),
            2.0 * PINCH_GESTURE_AMPLIFICATION,
        );
        assert!(input.sources().contains(CameraInteractionSources::PINCH));
        Ok(())
    }

    #[test]
    fn touch_adapter_resolves_to_orbit_input() -> TestResult {
        let mut app = test_app();
        let bindings = OrbitCamBindings::builder()
            .touch(Some(OrbitCamTouchBinding::OneFingerOrbit))
            .build()
            .map_err(|_| "bindings should validate")?;
        let camera = spawn_camera(app.world_mut(), bindings);
        route_to(&mut app, camera);
        app.insert_resource(OrbitCamTouchAdapterOverride(TouchGestures::OneFinger(
            OneFingerGestures {
                motion: Vec2::new(7.0, 8.0),
            },
        )));

        app.update();

        let input = camera_input(&app, camera)?;
        assert_eq!(input.orbit(), OrbitDelta::from(Vec2::new(7.0, 8.0)));
        assert!(input.sources().contains(CameraInteractionSources::TOUCH));
        Ok(())
    }

    #[test]
    fn keyboard_binding_resolves_to_smooth_zoom() -> TestResult {
        let mut app = test_app();
        let bindings = OrbitCamBindings::builder()
            .zoom(OrbitCamHeldBinding::new(KeyCode::Equal, KeyCode::ShiftLeft))
            .build()
            .map_err(|_| "bindings should validate")?;
        let camera = spawn_camera(app.world_mut(), bindings);
        route_to(&mut app, camera);
        {
            let mut keyboard = app.world_mut().resource_mut::<ButtonInput<KeyCode>>();
            keyboard.press(KeyCode::Equal);
            keyboard.press(KeyCode::ShiftLeft);
        }

        app.update();

        let input = camera_input(&app, camera)?;
        assert_f32_close(input.zoom_smooth().amount(), 1.0);
        assert!(input.sources().contains(CameraInteractionSources::KEYBOARD));
        Ok(())
    }

    #[test]
    fn gamepad_binding_resolves_to_orbit_input() -> TestResult {
        let mut app = test_app();
        let bindings = OrbitCamBindings::builder()
            .orbit(OrbitCamHeldBinding::new(
                GamepadAxis::LeftStickX,
                GamepadButton::LeftTrigger2,
            ))
            .gamepad(CameraInputGamepadSelectionPolicy::Active)
            .build()
            .map_err(|_| "bindings should validate")?;
        let camera = spawn_camera(app.world_mut(), bindings);
        route_to(&mut app, camera);
        let mut gamepad = Gamepad::default();
        gamepad.analog_mut().set(GamepadAxis::LeftStickX, 0.75);
        gamepad.analog_mut().set(GamepadButton::LeftTrigger2, 1.0);
        gamepad.digital_mut().press(GamepadButton::LeftTrigger2);
        app.world_mut().spawn(gamepad);

        app.update();

        let input = camera_input(&app, camera)?;
        assert_eq!(input.orbit().pixels(), Vec2::new(0.75, 0.0));
        assert!(input.sources().contains(CameraInteractionSources::GAMEPAD));
        Ok(())
    }

    #[test]
    fn cardinal_keyboard_binding_resolves_to_orbit_input() -> TestResult {
        let mut app = test_app();
        let bindings = OrbitCamBindings::builder()
            .orbit(OrbitCamInputBinding::cardinal_keys(
                KeyCode::ArrowUp,
                KeyCode::ArrowRight,
                KeyCode::ArrowDown,
                KeyCode::ArrowLeft,
            ))
            .build()
            .map_err(|_| "bindings should validate")?;
        let camera = spawn_camera(app.world_mut(), bindings);
        route_to(&mut app, camera);
        {
            let mut keyboard = app.world_mut().resource_mut::<ButtonInput<KeyCode>>();
            keyboard.press(KeyCode::ArrowRight);
            keyboard.press(KeyCode::ArrowUp);
        }

        app.update();

        let input = camera_input(&app, camera)?;
        assert_eq!(input.orbit().pixels(), Vec2::ONE);
        assert!(input.sources().contains(CameraInteractionSources::KEYBOARD));
        Ok(())
    }

    #[test]
    fn bidirectional_keyboard_binding_resolves_to_smooth_zoom() -> TestResult {
        let mut app = test_app();
        let bindings = OrbitCamBindings::builder()
            .zoom(OrbitCamInputBinding::bidirectional_keys(
                KeyCode::Equal,
                KeyCode::Minus,
            ))
            .build()
            .map_err(|_| "bindings should validate")?;
        let camera = spawn_camera(app.world_mut(), bindings);
        route_to(&mut app, camera);
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .press(KeyCode::Minus);

        app.update();

        let input = camera_input(&app, camera)?;
        assert_f32_close(input.zoom_smooth().amount(), -1.0);
        assert!(input.sources().contains(CameraInteractionSources::KEYBOARD));
        Ok(())
    }

    #[test]
    fn gamepad_axes2d_binding_resolves_to_orbit_input() -> TestResult {
        let mut app = test_app();
        let bindings = OrbitCamBindings::builder()
            .orbit(OrbitCamInputBinding::gamepad_axes_2d(
                GamepadAxis::RightStickX,
                GamepadAxis::RightStickY,
            ))
            .gamepad(CameraInputGamepadSelectionPolicy::Active)
            .build()
            .map_err(|_| "bindings should validate")?;
        let camera = spawn_camera(app.world_mut(), bindings);
        route_to(&mut app, camera);
        let mut gamepad = Gamepad::default();
        gamepad.analog_mut().set(GamepadAxis::RightStickX, 0.5);
        gamepad.analog_mut().set(GamepadAxis::RightStickY, -0.25);
        app.world_mut().spawn(gamepad);

        app.update();

        let input = camera_input(&app, camera)?;
        assert_eq!(input.orbit().pixels(), Vec2::new(0.5, -0.25));
        assert!(input.sources().contains(CameraInteractionSources::GAMEPAD));
        Ok(())
    }

    #[test]
    fn bidirectional_gamepad_buttons_resolve_to_smooth_zoom() -> TestResult {
        let mut app = test_app();
        let bindings = OrbitCamBindings::builder()
            .zoom(OrbitCamInputBinding::bidirectional_gamepad_buttons(
                GamepadButton::RightTrigger2,
                GamepadButton::LeftTrigger2,
            ))
            .gamepad(CameraInputGamepadSelectionPolicy::Active)
            .build()
            .map_err(|_| "bindings should validate")?;
        let camera = spawn_camera(app.world_mut(), bindings);
        route_to(&mut app, camera);
        let mut gamepad = Gamepad::default();
        gamepad.analog_mut().set(GamepadButton::LeftTrigger2, 0.4);
        gamepad.digital_mut().press(GamepadButton::LeftTrigger2);
        app.world_mut().spawn(gamepad);

        app.update();

        let input = camera_input(&app, camera)?;
        assert_f32_close(input.zoom_smooth().amount(), -0.4);
        assert!(input.sources().contains(CameraInteractionSources::GAMEPAD));
        Ok(())
    }

    #[test]
    fn manual_mode_bypasses_action_resolution() -> TestResult {
        let mut app = test_app();
        let camera = spawn_camera(app.world_mut(), OrbitCamManual);
        route_to(&mut app, camera);
        *app.world_mut().resource_mut::<AccumulatedMouseScroll>() = AccumulatedMouseScroll {
            unit:  MouseScrollUnit::Line,
            delta: Vec2::new(0.0, 3.0),
        };

        app.update();

        assert!(app.world().get::<OrbitCamInputContext>(camera).is_none());
        assert!(!camera_input(&app, camera)?.has_input());
        Ok(())
    }

    #[test]
    fn gated_camera_clears_previous_action_input() -> TestResult {
        let mut app = test_app();
        let camera = spawn_camera(app.world_mut(), OrbitCamPreset::SimpleMouse);
        route_to(&mut app, camera);
        *app.world_mut().resource_mut::<AccumulatedMouseScroll>() = AccumulatedMouseScroll {
            unit:  MouseScrollUnit::Line,
            delta: Vec2::new(0.0, 3.0),
        };
        app.update();
        assert!(camera_input(&app, camera)?.has_zoom());

        app.world_mut()
            .entity_mut(camera)
            .insert(CameraInputDisabled);
        *app.world_mut().resource_mut::<AccumulatedMouseScroll>() = AccumulatedMouseScroll {
            unit:  MouseScrollUnit::Line,
            delta: Vec2::new(0.0, 3.0),
        };
        app.update();

        assert!(!camera_input(&app, camera)?.has_input());
        Ok(())
    }
}
