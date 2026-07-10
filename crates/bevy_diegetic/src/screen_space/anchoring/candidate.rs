//! Candidate validation for screen-space panel attachments.

use bevy::platform::collections::HashMap;
use bevy::prelude::*;
use bevy::window::PrimaryWindow;
use hana_valence::AttachmentResolveCandidate;

use super::window;
use super::window::WindowResolveFailure;
use crate::panel::CoordinateSpace;
use crate::panel::DiegeticPanel;
use crate::panel::PanelAnchorOffset;
use crate::panel::PanelAttachmentAuthored;
use crate::panel::ResolvedScreenPanelPosition;

pub(super) fn classify_candidates(
    attachments: &Query<(Entity, &PanelAttachmentAuthored, &PanelAnchorOffset)>,
    panels: &Query<(Entity, &DiegeticPanel), With<ResolvedScreenPanelPosition>>,
    entities: &Query<()>,
    primary: &Query<Entity, With<PrimaryWindow>>,
    window_sizes: &HashMap<Entity, Vec2>,
) -> Vec<AttachmentResolveCandidate<AnchorResolveSkip>> {
    let mut candidates = Vec::new();
    for (source, attachment, _) in attachments {
        if panels.get(source).is_ok_and(|(_, panel)| {
            matches!(panel.coordinate_space(), CoordinateSpace::World { .. })
        }) {
            continue;
        }
        candidates.push(classify_candidate(
            source,
            *attachment,
            panels,
            entities,
            primary,
            window_sizes,
        ));
    }
    candidates
}

fn classify_candidate(
    source: Entity,
    attachment: PanelAttachmentAuthored,
    panels: &Query<(Entity, &DiegeticPanel), With<ResolvedScreenPanelPosition>>,
    entities: &Query<()>,
    primary: &Query<Entity, With<PrimaryWindow>>,
    window_sizes: &HashMap<Entity, Vec2>,
) -> AttachmentResolveCandidate<AnchorResolveSkip> {
    let target = attachment.target();
    match validate_candidate(source, attachment, panels, entities, primary, window_sizes) {
        Ok(()) => AttachmentResolveCandidate::Active {
            source,
            target,
            attachment: attachment.valence_relation(),
        },
        Err(reason) => AttachmentResolveCandidate::Skipped {
            source,
            target,
            reason,
        },
    }
}

fn validate_candidate(
    source: Entity,
    attachment: PanelAttachmentAuthored,
    panels: &Query<(Entity, &DiegeticPanel), With<ResolvedScreenPanelPosition>>,
    entities: &Query<()>,
    primary: &Query<Entity, With<PrimaryWindow>>,
    window_sizes: &HashMap<Entity, Vec2>,
) -> Result<(), AnchorResolveSkip> {
    let target = attachment.target();
    let Ok((_, source_panel)) = panels.get(source) else {
        return Err(AnchorResolveSkip::SourceWithoutPanel);
    };
    if source == target {
        return Err(AnchorResolveSkip::SelfAttachment);
    }
    if !entities.contains(target) {
        return Err(AnchorResolveSkip::TargetMissing);
    }
    let Ok((_, target_panel)) = panels.get(target) else {
        return Err(AnchorResolveSkip::TargetWithoutPanel);
    };
    let CoordinateSpace::Screen {
        window: source_window,
        ..
    } = source_panel.coordinate_space()
    else {
        return Err(AnchorResolveSkip::MixedCoordinateSpace);
    };
    let CoordinateSpace::Screen {
        window: target_window,
        ..
    } = target_panel.coordinate_space()
    else {
        return Err(AnchorResolveSkip::MixedCoordinateSpace);
    };
    let source_window =
        window::resolve_window(*source_window, primary, window_sizes).map_err(|failure| {
            match failure {
                WindowResolveFailure::Missing => AnchorResolveSkip::SourceWindowMissing,
                WindowResolveFailure::ZeroSized => AnchorResolveSkip::SourceWindowZeroSized,
            }
        })?;
    let target_window =
        window::resolve_window(*target_window, primary, window_sizes).map_err(|failure| {
            match failure {
                WindowResolveFailure::Missing => AnchorResolveSkip::TargetWindowMissing,
                WindowResolveFailure::ZeroSized => AnchorResolveSkip::TargetWindowZeroSized,
            }
        })?;
    if source_window != target_window {
        return Err(AnchorResolveSkip::CrossWindow);
    }

    Ok(())
}

/// Why a screen-space attachment did not resolve in the current frame.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, Reflect)]
pub(crate) enum AnchorResolveSkip {
    SourceWithoutPanel,
    TargetMissing,
    TargetWithoutPanel,
    SelfAttachment,
    SourceWindowMissing,
    SourceWindowZeroSized,
    TargetWindowMissing,
    TargetWindowZeroSized,
    CrossWindow,
    MixedCoordinateSpace,
    Cycle,
    BlockedByCycle,
    BlockedBySkippedDependency,
}
