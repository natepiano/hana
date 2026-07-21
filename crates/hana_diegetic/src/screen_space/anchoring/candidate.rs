//! Candidate validation for screen-space panel attachments.

use bevy::platform::collections::HashMap;
use bevy::prelude::*;
use bevy::window::WindowRef;
use hana_valence::AnchorId;
use hana_valence::AnchoredTo as ValenceAnchoredTo;
use hana_valence::AttachmentResolveCandidate;

use super::window;
use super::window::WindowResolveFailure;
use crate::panel::CoordinateSpace;
use crate::panel::DiegeticPanel;
use crate::panel::PanelAnchorOffset;
use crate::panel::PanelAttachmentAuthored;
use crate::panel::PanelSpace;
use crate::panel::WidgetOwnerLayout;
use crate::screen_space::CandidateQueries;
use crate::widgets::ScreenWidgetAnchorProxy;
use crate::widgets::ScreenWidgetAnchoredHere;
use crate::widgets::WidgetAnchorRect;
use crate::widgets::WidgetOf;

type WidgetTargetState<'a> = (
    Option<&'a WidgetOf>,
    Option<&'a WidgetAnchorRect>,
    Option<&'a ScreenWidgetAnchoredHere>,
    Option<&'a ScreenWidgetAnchorProxy>,
);

pub(super) fn classify_candidates(
    attachments: &Query<(Entity, &PanelAttachmentAuthored, &PanelAnchorOffset)>,
    queries: &CandidateQueries<'_, '_>,
    window_sizes: &HashMap<Entity, Vec2>,
) -> Vec<AttachmentResolveCandidate<AnchorResolveSkip>> {
    attachments
        .iter()
        .filter_map(|(source, attachment, _)| {
            classify_candidate(source, *attachment, queries, window_sizes)
        })
        .collect()
}

fn classify_candidate(
    source: Entity,
    attachment: PanelAttachmentAuthored,
    queries: &CandidateQueries<'_, '_>,
    window_sizes: &HashMap<Entity, Vec2>,
) -> Option<AttachmentResolveCandidate<AnchorResolveSkip>> {
    let target = attachment.target();
    let source_panel = queries.panels.get(source).ok().map(|(_, panel)| panel);
    if source_panel
        .is_some_and(|panel| matches!(panel.coordinate_space(), CoordinateSpace::World { .. }))
    {
        return None;
    }
    Some(
        match validate_candidate(source, attachment, queries, window_sizes) {
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
        },
    )
}

fn validate_candidate(
    source: Entity,
    attachment: PanelAttachmentAuthored,
    queries: &CandidateQueries<'_, '_>,
    window_sizes: &HashMap<Entity, Vec2>,
) -> Result<(), AnchorResolveSkip> {
    let target = attachment.target();
    let Ok((_, source_panel)) = queries.panels.get(source) else {
        return Err(AnchorResolveSkip::SourceWithoutPanel);
    };
    if source == target {
        return Err(AnchorResolveSkip::SelfAttachment);
    }
    if !queries.entities.contains(target) {
        return Err(AnchorResolveSkip::TargetMissing);
    }
    if queries.transforms.get(source).is_err() {
        return Err(AnchorResolveSkip::SourceTransformMissing);
    }
    let CoordinateSpace::Screen {
        window: source_window,
        ..
    } = source_panel.coordinate_space()
    else {
        return Err(AnchorResolveSkip::MixedCoordinateSpace);
    };
    let source_window = resolve_source_window(*source_window, queries, window_sizes)?;

    match (queries.panels.get(target), queries.widgets.get(target)) {
        (Ok((_, target_panel)), _) => {
            validate_panel_target(target_panel, target, source_window, queries, window_sizes)
        },
        (Err(_), Ok((widget_of, anchor_rect, demand, proxy))) => validate_widget_target(
            source,
            target,
            (widget_of, anchor_rect, demand, proxy),
            source_window,
            queries,
            window_sizes,
        ),
        (Err(_), Err(_)) => Err(AnchorResolveSkip::TargetWithoutPanel),
    }
}

fn validate_panel_target(
    target_panel: &DiegeticPanel,
    target: Entity,
    source_window: Entity,
    queries: &CandidateQueries<'_, '_>,
    window_sizes: &HashMap<Entity, Vec2>,
) -> Result<(), AnchorResolveSkip> {
    let CoordinateSpace::Screen {
        window: target_window,
        ..
    } = target_panel.coordinate_space()
    else {
        return Err(AnchorResolveSkip::MixedCoordinateSpace);
    };
    if queries.transforms.get(target).is_err() {
        return Err(AnchorResolveSkip::TargetTransformMissing);
    }
    let target_window = resolve_target_window(*target_window, queries, window_sizes)?;
    if source_window != target_window {
        return Err(AnchorResolveSkip::CrossWindow);
    }
    Ok(())
}

fn validate_widget_target(
    source: Entity,
    target: Entity,
    target_state: WidgetTargetState<'_>,
    source_window: Entity,
    queries: &CandidateQueries<'_, '_>,
    window_sizes: &HashMap<Entity, Vec2>,
) -> Result<(), AnchorResolveSkip> {
    let (widget_of, anchor_rect, demand, proxy) = target_state;
    let Some(widget_of) = widget_of else {
        return Err(AnchorResolveSkip::TargetOwnerMissing);
    };
    let Ok((_, owner_panel)) = queries.panels.get(widget_of.panel()) else {
        return Err(AnchorResolveSkip::TargetOwnerMissing);
    };
    let owner_layout = WidgetOwnerLayout::from(owner_panel);
    if owner_layout.panel_space() != PanelSpace::Screen {
        return Err(AnchorResolveSkip::MixedCoordinateSpace);
    }
    if anchor_rect.is_none()
        || queries.geometry.get(target).is_err()
        || proxy.is_none()
        || demand.is_none_or(|demand| !demand.contains(&source))
    {
        return Err(AnchorResolveSkip::TargetGeometryMissing);
    }
    if queries.transforms.get(widget_of.panel()).is_err() {
        return Err(AnchorResolveSkip::TargetTransformMissing);
    }
    let CoordinateSpace::Screen {
        window: target_window,
        ..
    } = owner_panel.coordinate_space()
    else {
        return Err(AnchorResolveSkip::MixedCoordinateSpace);
    };
    let target_window = resolve_target_window(*target_window, queries, window_sizes)?;
    if source_window != target_window {
        return Err(AnchorResolveSkip::CrossWindow);
    }
    Ok(())
}

pub(super) fn proxy_candidates(
    queries: &CandidateQueries<'_, '_>,
) -> Vec<AttachmentResolveCandidate<AnchorResolveSkip>> {
    queries
        .proxy_candidates
        .iter()
        .filter_map(|(widget, widget_of, anchor_rect, demand)| {
            let owner = widget_of.panel();
            let (_, owner_panel) = queries.panels.get(owner).ok()?;
            let owner_layout = WidgetOwnerLayout::from(owner_panel);
            if demand.is_empty()
                || anchor_rect.space() != PanelSpace::Screen
                || owner_layout.panel_space() != PanelSpace::Screen
                || queries.transforms.get(owner).is_err()
            {
                return None;
            }
            Some(AttachmentResolveCandidate::Active {
                source:     widget,
                target:     owner,
                attachment: ValenceAnchoredTo::new(owner, AnchorId::Center, AnchorId::Center),
            })
        })
        .collect()
}

fn resolve_source_window(
    window_ref: WindowRef,
    queries: &CandidateQueries<'_, '_>,
    window_sizes: &HashMap<Entity, Vec2>,
) -> Result<Entity, AnchorResolveSkip> {
    window::resolve_window(window_ref, &queries.primary, window_sizes)
        .map(|(entity, _)| entity)
        .map_err(|failure| match failure {
            WindowResolveFailure::Missing => AnchorResolveSkip::SourceWindowMissing,
            WindowResolveFailure::ZeroSized => AnchorResolveSkip::SourceWindowZeroSized,
        })
}

fn resolve_target_window(
    window_ref: WindowRef,
    queries: &CandidateQueries<'_, '_>,
    window_sizes: &HashMap<Entity, Vec2>,
) -> Result<Entity, AnchorResolveSkip> {
    window::resolve_window(window_ref, &queries.primary, window_sizes)
        .map(|(entity, _)| entity)
        .map_err(|failure| match failure {
            WindowResolveFailure::Missing => AnchorResolveSkip::TargetWindowMissing,
            WindowResolveFailure::ZeroSized => AnchorResolveSkip::TargetWindowZeroSized,
        })
}

/// Why a screen-space attachment did not resolve in the current frame.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, Reflect)]
pub(crate) enum AnchorResolveSkip {
    SourceWithoutPanel,
    SourceGeometryMissing,
    SourceTransformMissing,
    TargetMissing,
    TargetWithoutPanel,
    TargetOwnerMissing,
    TargetGeometryMissing,
    TargetTransformMissing,
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
