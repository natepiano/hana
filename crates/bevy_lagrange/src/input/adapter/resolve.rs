//! Reads the enhanced-input action state and writes the per-camera [`OrbitCamInput`].
//!
//! [`resolve_actions_into_orbit_cam_input`] runs each frame in
//! `OrbitCamInputInternalSet::ActionResolution`. For each routed, non-gated, non-manual
//! camera it samples the active engagement states, picks the source mask appropriate for
//! the engaged held bindings, and accumulates orbit / pan / zoom into the camera's
//! [`OrbitCamInput`] component — both from the user-bound actions and from the adapter-
//! synthesized actions written by [`super::inject`].
//!
//! Types (private — only used inside this module):
//! - [`Vec2ActionQueries`] / [`F32ActionQueries`] / [`BoolActionQueries`] — `SystemParam` bundles
//!   holding the per-action `Query<&Action<...>>` reads.
//! - [`HeldEngagement`] — `Engaged` / `Idle` enum derived from each engagement bool, used to
//!   short-circuit source selection.

use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use bevy_enhanced_input::prelude::Action;
use bevy_enhanced_input::prelude::InputAction;
use bevy_enhanced_input::prelude::ModKeys;
use bevy_enhanced_input::prelude::TriggerState;

use super::AdapterScale;
use super::inject::OrbitCamAdapterFrameSources;
use super::install::OrbitCamInputActionEntities;
use super::install::OrbitCamInstalledBindings;
use crate::input::CameraInteractionSources;
use crate::input::ControlSpeed;
use crate::input::HeldActionBindingEntry;
use crate::input::HeldCameraAction;
use crate::input::OrbitCamBindings;
use crate::input::OrbitCamInput;
use crate::input::OrbitCamInputContextGated;
use crate::input::OrbitCamManual;
use crate::input::OrbitCamOrbitAction;
use crate::input::OrbitCamPanAction;
use crate::input::OrbitCamSlowModeLatches;
use crate::input::OrbitCamSlowModeState;
use crate::input::OrbitCamZoomCoarseAction;
use crate::input::OrbitCamZoomSmoothAction;
use crate::input::ResolvedOrbitCamInputRoute;
use crate::input::actions::OrbitCamAdapterOrbitAction;
use crate::input::actions::OrbitCamAdapterPanAction;
use crate::input::actions::OrbitCamAdapterZoomCoarseAction;
use crate::input::actions::OrbitCamAdapterZoomSmoothAction;
use crate::input::actions::OrbitCamOrbitEngagedAction;
use crate::input::actions::OrbitCamOrbitSlowAction;
use crate::input::actions::OrbitCamPanEngagedAction;
use crate::input::actions::OrbitCamPanSlowAction;
use crate::input::actions::OrbitCamZoomEngagedAction;
use crate::input::actions::OrbitCamZoomSmoothSlowAction;
use crate::input::routing;

#[allow(
    clippy::too_many_lines,
    reason = "the resolver keeps related enhanced-input query reads in one scheduling system"
)]
pub(super) fn resolve_actions_into_orbit_cam_input(
    route: Res<ResolvedOrbitCamInputRoute>,
    mut slow_latches: ResMut<OrbitCamSlowModeLatches>,
    mut cameras: Query<
        (
            Entity,
            &OrbitCamInstalledBindings,
            &OrbitCamInputActionEntities,
            &OrbitCamAdapterFrameSources,
            Option<&OrbitCamInputContextGated>,
            Option<&mut OrbitCamSlowModeState>,
            &mut OrbitCamInput,
        ),
        Without<OrbitCamManual>,
    >,
    vec2_actions: Vec2ActionQueries,
    f32_actions: F32ActionQueries,
    bool_actions: BoolActionQueries,
    states: Query<&TriggerState>,
    keyboard: Option<Res<ButtonInput<KeyCode>>>,
    mouse_buttons: Option<Res<ButtonInput<MouseButton>>>,
) {
    for (camera, bindings, actions, frame_sources, gated, slow_mode_state, mut input) in
        &mut cameras
    {
        input.clear();
        if route.routed_camera() != Some(camera)
            || route.metrics_for(camera).is_none()
            || route
                .blockers_for(camera)
                .is_some_and(crate::input::OrbitCamInputBlockers::is_blocked)
            || gated.is_some_and(|gated| !gated.context_gate.is_allowed())
        {
            continue;
        }

        if states
            .get(actions.slow_mode_toggle)
            .is_ok_and(|state| *state == TriggerState::Fired)
        {
            routing::toggle_slow_mode_latch(&mut slow_latches, camera);
        }
        let slow_mode_active = routing::is_slow_mode_active(&slow_latches, camera);
        if let Some(mut state) = slow_mode_state {
            state.set_active(slow_mode_active);
        }

        let orbit_engaged = bool_action_active(actions.orbit_engaged, &bool_actions.orbit, &states);
        let pan_engaged = bool_action_active(actions.pan_engaged, &bool_actions.pan, &states);
        let zoom_engaged = bool_action_active(actions.zoom_engaged, &bool_actions.zoom, &states);
        let pan_overrides_orbit =
            pan_overrides_orbit(&bindings.0, keyboard.as_deref(), mouse_buttons.as_deref());
        let adapter_scale = AdapterScale::from_bindings(&bindings.0, slow_mode_active);
        let orbit_sources = held_sources_for_state(
            HeldEngagement::from(orbit_engaged),
            bindings.0.orbit().enabled_entries(),
            actions.orbit_sources,
            keyboard.as_deref(),
            mouse_buttons.as_deref(),
        );
        let pan_sources = held_sources_for_state(
            HeldEngagement::from(pan_engaged),
            bindings.0.pan().enabled_entries(),
            actions.pan_sources,
            keyboard.as_deref(),
            mouse_buttons.as_deref(),
        );
        let zoom_smooth_sources = held_sources_for_state(
            HeldEngagement::from(zoom_engaged),
            bindings.0.zoom_smooth().enabled_entries(),
            actions.zoom_smooth_sources,
            keyboard.as_deref(),
            mouse_buttons.as_deref(),
        );

        if orbit_engaged && !pan_overrides_orbit {
            let normal = action_value(actions.orbit, &vec2_actions.orbit);
            let slow = action_value(actions.orbit_slow, &vec2_actions.orbit_slow);
            input.orbit_pixels_with_sources(adapter_scale.vec2(normal + slow), orbit_sources);
            input.set_orbit_speed(vec2_speed(slow));
        }
        if pan_engaged {
            let normal = action_value(actions.pan, &vec2_actions.pan);
            let slow = action_value(actions.pan_slow, &vec2_actions.pan_slow);
            input.pan_pixels_with_sources(adapter_scale.vec2(normal + slow), pan_sources);
            input.set_pan_speed(vec2_speed(slow));
        }
        if zoom_engaged {
            let normal = action_value(actions.zoom_smooth, &f32_actions.zoom_smooth);
            let slow = action_value(actions.zoom_smooth_slow, &f32_actions.zoom_smooth_slow);
            input.zoom_smooth_with_sources(adapter_scale.f32(normal + slow), zoom_smooth_sources);
            input.set_zoom_speed(f32_speed(slow));
        }
        if action_state_active(actions.zoom_coarse, &states) {
            input.zoom_coarse_with_sources(
                adapter_scale.f32(action_value(actions.zoom_coarse, &f32_actions.zoom_coarse)),
                actions.zoom_coarse_sources,
            );
        }

        if action_state_active(actions.adapter_orbit, &states) {
            input.orbit_pixels_with_sources(
                adapter_scale.vec2(action_value(
                    actions.adapter_orbit,
                    &vec2_actions.adapter_orbit,
                )),
                frame_sources.orbit,
            );
        }
        if action_state_active(actions.adapter_pan, &states) {
            input.pan_pixels_with_sources(
                adapter_scale.vec2(action_value(actions.adapter_pan, &vec2_actions.adapter_pan)),
                frame_sources.pan,
            );
        }
        if action_state_active(actions.adapter_zoom_coarse, &states) {
            input.zoom_coarse_with_sources(
                adapter_scale.f32(action_value(
                    actions.adapter_zoom_coarse,
                    &f32_actions.adapter_zoom_coarse,
                )),
                frame_sources.zoom_coarse,
            );
        }
        if action_state_active(actions.adapter_zoom_smooth, &states) {
            input.zoom_smooth_with_sources(
                adapter_scale.f32(action_value(
                    actions.adapter_zoom_smooth,
                    &f32_actions.adapter_zoom_smooth,
                )),
                frame_sources.zoom_smooth,
            );
        }
    }
}

#[derive(SystemParam)]
pub(super) struct Vec2ActionQueries<'w, 's> {
    orbit:         Query<'w, 's, &'static Action<OrbitCamOrbitAction>>,
    orbit_slow:    Query<'w, 's, &'static Action<OrbitCamOrbitSlowAction>>,
    pan:           Query<'w, 's, &'static Action<OrbitCamPanAction>>,
    pan_slow:      Query<'w, 's, &'static Action<OrbitCamPanSlowAction>>,
    adapter_orbit: Query<'w, 's, &'static Action<OrbitCamAdapterOrbitAction>>,
    adapter_pan:   Query<'w, 's, &'static Action<OrbitCamAdapterPanAction>>,
}

#[derive(SystemParam)]
pub(super) struct F32ActionQueries<'w, 's> {
    zoom_coarse:         Query<'w, 's, &'static Action<OrbitCamZoomCoarseAction>>,
    zoom_smooth:         Query<'w, 's, &'static Action<OrbitCamZoomSmoothAction>>,
    zoom_smooth_slow:    Query<'w, 's, &'static Action<OrbitCamZoomSmoothSlowAction>>,
    adapter_zoom_coarse: Query<'w, 's, &'static Action<OrbitCamAdapterZoomCoarseAction>>,
    adapter_zoom_smooth: Query<'w, 's, &'static Action<OrbitCamAdapterZoomSmoothAction>>,
}

#[derive(SystemParam)]
pub(super) struct BoolActionQueries<'w, 's> {
    orbit: Query<'w, 's, &'static Action<OrbitCamOrbitEngagedAction>>,
    pan:   Query<'w, 's, &'static Action<OrbitCamPanEngagedAction>>,
    zoom:  Query<'w, 's, &'static Action<OrbitCamZoomEngagedAction>>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum HeldEngagement {
    Engaged,
    Idle,
}

impl From<bool> for HeldEngagement {
    fn from(engaged: bool) -> Self { if engaged { Self::Engaged } else { Self::Idle } }
}

fn held_sources_for_state<'a, A: HeldCameraAction + 'a>(
    engagement: HeldEngagement,
    entries: impl IntoIterator<Item = &'a HeldActionBindingEntry<A>>,
    fallback: CameraInteractionSources,
    keyboard: Option<&ButtonInput<KeyCode>>,
    mouse_buttons: Option<&ButtonInput<MouseButton>>,
) -> CameraInteractionSources {
    if matches!(engagement, HeldEngagement::Idle) {
        return CameraInteractionSources::NONE;
    }

    let active_sources = entries
        .into_iter()
        .filter(|entry| {
            entry
                .engagement_descriptor()
                .enabled_is_active(keyboard, mouse_buttons)
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
    bindings.pan().enabled_entries().any(|pan| {
        let Some((pan_button, pan_mod_keys)) = pan
            .engagement_descriptor()
            .enabled_mouse_button_engagement()
        else {
            return false;
        };
        if !pan
            .engagement_descriptor()
            .enabled_is_active(keyboard, mouse_buttons)
        {
            return false;
        }
        bindings.orbit().enabled_entries().any(|orbit| {
            let Some((orbit_button, orbit_mod_keys)) = orbit
                .engagement_descriptor()
                .enabled_mouse_button_engagement()
            else {
                return false;
            };
            pan_button == orbit_button
                && mod_key_count(pan_mod_keys) > mod_key_count(orbit_mod_keys)
                && orbit
                    .engagement_descriptor()
                    .enabled_is_active(keyboard, mouse_buttons)
        })
    })
}

fn mod_key_count(mod_keys: ModKeys) -> usize { mod_keys.iter_names().count() }

fn bool_action_active<A: InputAction<Output = bool>>(
    action: Entity,
    actions: &Query<&Action<A>>,
    states: &Query<&TriggerState>,
) -> bool {
    action_state_active(action, states) && actions.get(action).is_ok_and(|action| **action)
}

/// The active speed for a held kind: any contribution from the slow motion
/// action means the slow variant is engaged. The normal and slow actions are
/// mutually exclusive (BEI's gate conditions ensure only one carries a value).
fn vec2_speed(slow: Vec2) -> ControlSpeed {
    if slow == Vec2::ZERO {
        ControlSpeed::Normal
    } else {
        ControlSpeed::Slow
    }
}

fn f32_speed(slow: f32) -> ControlSpeed {
    if slow == 0.0 {
        ControlSpeed::Normal
    } else {
        ControlSpeed::Slow
    }
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
