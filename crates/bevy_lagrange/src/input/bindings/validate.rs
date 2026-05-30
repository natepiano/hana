//! Validates a [`super::OrbitCamBindingsDescriptor`] and converts it into a
//! [`super::OrbitCamBindings`].
//!
//! [`validate_bindings`] is the single entry point called by both
//! [`super::OrbitCamBindingsBuilder::build`] and the [`TryFrom`] impl on
//! [`super::OrbitCamBindings`]. The remaining functions are private helpers that walk the
//! per-action descriptor entries, enforce the held/impulse invariants, and lower the
//! descriptor entries into [`super::action_set::HeldActionBindingEntry`] /
//! [`super::action_set::ActionBindingEntry`] storage.

use std::marker::PhantomData;

use super::OrbitCamBindings;
use super::OrbitCamBindingsDescriptor;
use super::action_set::ActionBindingEntry;
use super::action_set::ActionBindingSet;
use super::action_set::BindingEngagement;
use super::action_set::HeldActionBindingEntry;
use super::action_set::HeldActionBindingSet;
use super::action_set::OrbitCamOrbitActionBindings;
use super::action_set::OrbitCamPanActionBindings;
use super::action_set::OrbitCamZoomCoarseActionBindings;
use super::action_set::OrbitCamZoomSmoothActionBindings;
use super::descriptor::ActionBindingDescriptor;
use super::descriptor::HeldBindingDescriptor;
use super::descriptor::InputBindingDescriptor;
use super::descriptor::InputBindingEntry;
use super::error::OrbitCamBindingsError;
use super::held_binding::BindingGates;
use super::held_binding::OrbitCamGatePolarity;
use crate::input::CameraSemanticAction;
use crate::input::HeldCameraAction;
use crate::input::ImpulseCameraAction;
use crate::input::constants::ORBIT_ACTION_NAME;
use crate::input::constants::PAN_ACTION_NAME;
use crate::input::constants::ZOOM_COARSE_ACTION_NAME;
use crate::input::constants::ZOOM_SMOOTH_ACTION_NAME;

/// Validates and builds `OrbitCamBindings` from a descriptor.
///
/// # Errors
///
/// Returns [`OrbitCamBindingsError`] when any binding invariant fails.
pub fn validate_bindings(
    descriptor: &OrbitCamBindingsDescriptor,
) -> Result<OrbitCamBindings, OrbitCamBindingsError> {
    validate_held_entries(ORBIT_ACTION_NAME, &descriptor.orbit)?;
    validate_held_entries(PAN_ACTION_NAME, &descriptor.pan)?;
    validate_held_entries(ZOOM_SMOOTH_ACTION_NAME, &descriptor.zoom_smooth)?;
    validate_impulse_entries(ZOOM_COARSE_ACTION_NAME, &descriptor.zoom_coarse)?;

    Ok(OrbitCamBindings {
        orbit:            OrbitCamOrbitActionBindings(held_descriptors_to_set(
            ORBIT_ACTION_NAME,
            &descriptor.orbit,
        )?),
        pan:              OrbitCamPanActionBindings(held_descriptors_to_set(
            PAN_ACTION_NAME,
            &descriptor.pan,
        )?),
        zoom_smooth:      OrbitCamZoomSmoothActionBindings(held_descriptors_to_set(
            ZOOM_SMOOTH_ACTION_NAME,
            &descriptor.zoom_smooth,
        )?),
        zoom_coarse:      OrbitCamZoomCoarseActionBindings(ActionBindingSet {
            entries: descriptor
                .zoom_coarse
                .iter()
                .map(action_descriptor_to_entry)
                .collect(),
            action:  PhantomData,
        }),
        trackpad_orbit:   descriptor.trackpad_orbit.clone(),
        trackpad_pan:     descriptor.trackpad_pan.clone(),
        trackpad_zoom:    descriptor.trackpad_zoom.clone(),
        mouse_wheel_zoom: descriptor.mouse_wheel_zoom,
        pinch_zoom:       descriptor.pinch_zoom,
        touch:            descriptor.touch,
        gamepad:          descriptor.gamepad,
        zoom_direction:   descriptor.zoom_direction,
        button_drag_zoom: descriptor.button_drag_zoom,
        profile:          descriptor.profile,
    })
}

fn validate_held_entries(
    action: &'static str,
    entries: &[HeldBindingDescriptor],
) -> Result<(), OrbitCamBindingsError> {
    for entry in entries {
        if entry.sources.is_empty() {
            return Err(OrbitCamBindingsError::MissingSources);
        }
        if entry.engagement.is_none() {
            return Err(OrbitCamBindingsError::HeldMotionMissingEngagement { action });
        }
        if entry.sources != entry.engagement_sources {
            return Err(OrbitCamBindingsError::HeldSourceMismatch { action });
        }
        if entry.motion.is_empty()
            || entry
                .engagement
                .as_ref()
                .is_some_and(super::descriptor::InputBindingDescriptor::is_empty)
        {
            return Err(OrbitCamBindingsError::MissingSources);
        }
        validate_gates(action, &entry.gates)?;
        validate_descriptor_entries(&entry.motion)?;
        if let Some(engagement) = &entry.engagement {
            validate_descriptor_entries(engagement)?;
        }
    }
    Ok(())
}

fn validate_impulse_entries(
    action: &'static str,
    entries: &[ActionBindingDescriptor],
) -> Result<(), OrbitCamBindingsError> {
    for entry in entries {
        if entry.sources.is_empty() || entry.binding.is_empty() {
            return Err(OrbitCamBindingsError::MissingSources);
        }
        if entry.engagement == BindingEngagement::Held {
            return Err(OrbitCamBindingsError::ImpulseEngagement { action });
        }
        validate_descriptor_entries(&entry.binding)?;
    }
    Ok(())
}

fn validate_gates(action: &'static str, gates: &BindingGates) -> Result<(), OrbitCamBindingsError> {
    for gate in gates.entries() {
        let opposite = match gate.polarity {
            OrbitCamGatePolarity::Required => OrbitCamGatePolarity::Blocked,
            OrbitCamGatePolarity::Blocked => OrbitCamGatePolarity::Required,
        };
        if gates
            .entries()
            .iter()
            .any(|candidate| candidate.input == gate.input && candidate.polarity == opposite)
        {
            return Err(OrbitCamBindingsError::ContradictoryGate { action });
        }
    }
    Ok(())
}

fn validate_descriptor_entries(
    descriptor: &InputBindingDescriptor,
) -> Result<(), OrbitCamBindingsError> {
    for entry in descriptor.entries_slice() {
        validate_entry(entry)?;
    }
    Ok(())
}

fn validate_entry(entry: &InputBindingEntry) -> Result<(), OrbitCamBindingsError> {
    let modifiers = entry.modifiers();
    if modifiers.scale().is_some_and(|scale| !scale.is_finite()) {
        return Err(OrbitCamBindingsError::InvalidScale);
    }
    let Some(dead_zone) = modifiers.dead_zone() else {
        return Ok(());
    };
    let lower = dead_zone.lower_threshold;
    let upper = dead_zone.upper_threshold;
    if !lower.is_finite() || !upper.is_finite() || lower < 0.0 || upper > 1.0 || lower >= upper {
        return Err(OrbitCamBindingsError::InvalidDeadZone);
    }
    Ok(())
}

fn held_descriptors_to_set<A: HeldCameraAction, E: CameraSemanticAction>(
    action: &'static str,
    descriptors: &[HeldBindingDescriptor],
) -> Result<HeldActionBindingSet<A, E>, OrbitCamBindingsError> {
    Ok(HeldActionBindingSet {
        entries: descriptors
            .iter()
            .map(|descriptor| held_descriptor_to_entry(action, descriptor))
            .collect::<Result<Vec<_>, _>>()?,
        action:  PhantomData,
    })
}

fn held_descriptor_to_entry<A: HeldCameraAction>(
    action: &'static str,
    descriptor: &HeldBindingDescriptor,
) -> Result<HeldActionBindingEntry<A>, OrbitCamBindingsError> {
    let engagement = descriptor
        .engagement
        .clone()
        .ok_or(OrbitCamBindingsError::HeldMotionMissingEngagement { action })?;

    Ok(HeldActionBindingEntry {
        motion: descriptor.motion.clone(),
        engagement,
        gates: descriptor.gates.clone(),
        sources: descriptor.sources,
        route: descriptor.route,
        speed: descriptor.speed,
        action: PhantomData,
    })
}

fn action_descriptor_to_entry<A: ImpulseCameraAction>(
    descriptor: &ActionBindingDescriptor,
) -> ActionBindingEntry<A> {
    ActionBindingEntry {
        binding:    descriptor.binding.clone(),
        sources:    descriptor.sources,
        route:      descriptor.route,
        engagement: BindingEngagement::Impulse,
        action:     PhantomData,
    }
}
