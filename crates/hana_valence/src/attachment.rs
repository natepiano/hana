//! Dependency ordering for pre-classified attachment relations.

use std::collections::VecDeque;
use std::fmt::Debug;
use std::hash::Hash;

use bevy_ecs::entity::Entity;
use bevy_ecs::prelude::Resource;
use bevy_platform::collections::HashMap;
use bevy_platform::collections::HashSet;

use crate::AnchoredTo;

/// Attachment edge after consumer-owned validation.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum AttachmentResolveCandidate<R> {
    /// Edge can be resolved by the consumer's coordinate-space adapter.
    Active {
        /// Entity whose transform or pose will be written.
        source:     Entity,
        /// Entity that provides the target anchor.
        target:     Entity,
        /// Stored relationship payload for the source entity.
        attachment: AnchoredTo,
    },
    /// Edge is owned by this resolver but invalid for this frame.
    Skipped {
        /// Entity that cannot be resolved this frame.
        source: Entity,
        /// Entity that the source attempted to target.
        target: Entity,
        /// Consumer-specific skip reason.
        reason: R,
    },
}

/// Consumer-provided skip reasons used by the dependency resolver.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AttachmentResolveReasons<R> {
    /// Reason recorded when a source depends on an already-skipped target.
    pub blocked_by_skipped_dependency: R,
    /// Reason recorded for an entity that participates in an attachment cycle.
    pub cycle:                         R,
    /// Reason recorded for an entity blocked by an attachment cycle.
    pub blocked_by_cycle:              R,
}

/// Coordinate-space-specific work requested by the attachment resolver.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum AttachmentResolveAction {
    /// Place `source` from `target` using `attachment`.
    Place {
        /// Entity whose transform or pose should be written.
        source:     Entity,
        /// Entity that provides the target anchor.
        target:     Entity,
        /// Stored relationship payload for the source entity.
        attachment: AnchoredTo,
    },
    /// Restore or keep the source entity's fallback placement.
    Fallback {
        /// Entity that should use consumer-owned fallback placement.
        source: Entity,
    },
}

/// Resolves active candidates in dependency order and reports skipped edges.
///
/// Consumers classify candidates as [`AttachmentResolveCandidate::Active`] or
/// [`AttachmentResolveCandidate::Skipped`] before calling this function. The
/// resolver owns dependency ordering, fallback dispatch, cycle reporting, and
/// diagnostics accumulation.
pub fn resolve_attachments<R, F>(
    candidates: Vec<AttachmentResolveCandidate<R>>,
    reasons: AttachmentResolveReasons<R>,
    diagnostics: &mut AttachmentResolveDiagnostics<R>,
    mut handle: F,
) where
    R: Copy + Debug + Eq + Hash + Send + Sync + 'static,
    F: FnMut(AttachmentResolveAction) -> Result<(), R>,
{
    diagnostics.begin_frame();

    let mut graph = AttachmentGraph::default();
    let mut states = HashMap::default();
    for candidate in candidates {
        match candidate {
            AttachmentResolveCandidate::Active {
                source,
                target,
                attachment,
            } => graph.add(source, target, attachment),
            AttachmentResolveCandidate::Skipped {
                source,
                target,
                reason,
            } => {
                states.insert(source, AttachmentResolveState::Skipped);
                diagnostics.record(source, target, reason);
                apply_fallback(source, &mut handle);
            },
        }
    }

    graph.resolve(&mut states, reasons, diagnostics, &mut handle);
    graph.mark_unresolved_cycles(&mut states, reasons, diagnostics, &mut handle);
}

/// Attachment dependency graph built from active candidates.
#[derive(Default)]
struct AttachmentGraph {
    adjacency:   HashMap<Entity, Vec<Entity>>,
    attachments: HashMap<Entity, AnchoredTo>,
    indegree:    HashMap<Entity, usize>,
    target_of:   HashMap<Entity, Entity>,
}

impl AttachmentGraph {
    fn add(&mut self, source: Entity, target: Entity, attachment: AnchoredTo) {
        self.adjacency.entry(target).or_default().push(source);
        self.attachments.insert(source, attachment);
        self.target_of.insert(source, target);
        *self.indegree.entry(source).or_default() += 1;
        self.indegree.entry(target).or_default();
    }

    fn resolve<R, F>(
        &mut self,
        states: &mut HashMap<Entity, AttachmentResolveState>,
        reasons: AttachmentResolveReasons<R>,
        diagnostics: &mut AttachmentResolveDiagnostics<R>,
        handle: &mut F,
    ) where
        R: Copy + Debug + Eq + Hash + Send + Sync + 'static,
        F: FnMut(AttachmentResolveAction) -> Result<(), R>,
    {
        let mut queue = VecDeque::new();
        for (&entity, &indegree) in &self.indegree {
            if indegree == 0 {
                queue.push_back(entity);
            }
        }

        while let Some(entity) = queue.pop_front() {
            let state = states
                .get(&entity)
                .copied()
                .unwrap_or(AttachmentResolveState::Configured);
            self.resolve_children(
                entity,
                state,
                states,
                reasons,
                diagnostics,
                handle,
                &mut queue,
            );
        }
    }

    fn resolve_children<R, F>(
        &mut self,
        target: Entity,
        target_state: AttachmentResolveState,
        states: &mut HashMap<Entity, AttachmentResolveState>,
        reasons: AttachmentResolveReasons<R>,
        diagnostics: &mut AttachmentResolveDiagnostics<R>,
        handle: &mut F,
        queue: &mut VecDeque<Entity>,
    ) where
        R: Copy + Debug + Eq + Hash + Send + Sync + 'static,
        F: FnMut(AttachmentResolveAction) -> Result<(), R>,
    {
        let children = self.adjacency.get(&target).cloned().unwrap_or_default();
        for child in children {
            match target_state {
                AttachmentResolveState::Skipped => {
                    let reason = reasons.blocked_by_skipped_dependency;
                    states.insert(child, AttachmentResolveState::Skipped);
                    diagnostics.record(child, target, reason);
                    apply_fallback(child, handle);
                },
                AttachmentResolveState::Configured | AttachmentResolveState::Resolved => {
                    self.resolve_child_position(child, target, states, diagnostics, handle);
                },
            }
            if let Some(indegree) = self.indegree.get_mut(&child) {
                *indegree = indegree.saturating_sub(1);
                if *indegree == 0 {
                    queue.push_back(child);
                }
            }
        }
    }

    fn resolve_child_position<R, F>(
        &self,
        child: Entity,
        target: Entity,
        states: &mut HashMap<Entity, AttachmentResolveState>,
        diagnostics: &mut AttachmentResolveDiagnostics<R>,
        handle: &mut F,
    ) where
        R: Copy + Debug + Eq + Hash + Send + Sync + 'static,
        F: FnMut(AttachmentResolveAction) -> Result<(), R>,
    {
        let Some(attachment) = self.attachments.get(&child).copied() else {
            return;
        };
        match handle(AttachmentResolveAction::Place {
            source: child,
            target,
            attachment,
        }) {
            Ok(()) => {
                states.insert(child, AttachmentResolveState::Resolved);
            },
            Err(reason) => {
                states.insert(child, AttachmentResolveState::Skipped);
                diagnostics.record(child, target, reason);
                apply_fallback(child, handle);
            },
        }
    }

    fn mark_unresolved_cycles<R, F>(
        &self,
        states: &mut HashMap<Entity, AttachmentResolveState>,
        reasons: AttachmentResolveReasons<R>,
        diagnostics: &mut AttachmentResolveDiagnostics<R>,
        handle: &mut F,
    ) where
        R: Copy + Debug + Eq + Hash + Send + Sync + 'static,
        F: FnMut(AttachmentResolveAction) -> Result<(), R>,
    {
        let remaining = self.remaining_nodes();
        let cycle_members = cycle_members(&remaining, &self.target_of);
        for entity in remaining {
            let reason = if cycle_members.contains(&entity) {
                reasons.cycle
            } else {
                reasons.blocked_by_cycle
            };
            let target = self.target_of.get(&entity).copied().unwrap_or(entity);
            states.insert(entity, AttachmentResolveState::Skipped);
            diagnostics.record(entity, target, reason);
            apply_fallback(entity, handle);
        }
    }

    fn remaining_nodes(&self) -> HashSet<Entity> {
        let mut remaining = HashSet::default();
        for (&entity, &indegree) in &self.indegree {
            if indegree > 0 {
                remaining.insert(entity);
            }
        }
        remaining
    }
}

fn cycle_members(
    remaining: &HashSet<Entity>,
    target_of: &HashMap<Entity, Entity>,
) -> HashSet<Entity> {
    let mut cycle_members = HashSet::default();
    for &start in remaining {
        let mut path = Vec::new();
        let mut seen: HashMap<Entity, usize> = HashMap::default();
        let mut current = start;
        loop {
            if let Some(&index) = seen.get(&current) {
                for &entity in &path[index..] {
                    cycle_members.insert(entity);
                }
                break;
            }
            if !remaining.contains(&current) {
                break;
            }
            seen.insert(current, path.len());
            path.push(current);
            let Some(&target) = target_of.get(&current) else {
                break;
            };
            current = target;
        }
    }
    cycle_members
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum AttachmentResolveState {
    Configured,
    Resolved,
    Skipped,
}

fn apply_fallback<R>(
    source: Entity,
    handle: &mut impl FnMut(AttachmentResolveAction) -> Result<(), R>,
) {
    let _ = handle(AttachmentResolveAction::Fallback { source });
}

/// Bounded history of attachment resolution failures.
#[derive(Resource, Debug)]
pub struct AttachmentResolveDiagnostics<R: Send + Sync + 'static> {
    current_frame: u64,
    entries:       VecDeque<AttachmentResolveDiagnostic<R>>,
    capacity:      usize,
}

impl<R: Send + Sync + 'static> AttachmentResolveDiagnostics<R> {
    /// Default number of diagnostic entries retained in insertion order.
    pub const DEFAULT_CAPACITY: usize = 128;

    const fn begin_frame(&mut self) { self.current_frame = self.current_frame.saturating_add(1); }

    fn record(&mut self, source: Entity, target: Entity, reason: R)
    where
        R: Copy + Debug + Eq,
    {
        if let Some(entry) = self.entries.iter_mut().find(|entry| {
            entry.source == source && entry.target == target && entry.reason == reason
        }) {
            entry.last_seen_frame = self.current_frame;
            entry.count = entry.count.saturating_add(1);
            tracing::warn!(
                source = ?source,
                target = ?target,
                reason = ?reason,
                count = entry.count,
                "attachment skip repeated"
            );
            return;
        }

        self.entries.push_back(AttachmentResolveDiagnostic {
            source,
            target,
            reason,
            first_seen_frame: self.current_frame,
            last_seen_frame: self.current_frame,
            count: 1,
        });
        while self.entries.len() > self.capacity {
            self.entries.pop_front();
        }
    }

    /// Iterates over every retained diagnostic entry.
    pub fn entries(&self) -> impl Iterator<Item = &AttachmentResolveDiagnostic<R>> {
        self.entries.iter()
    }

    /// Iterates over diagnostic entries recorded in the current resolve frame.
    pub fn current(&self) -> impl Iterator<Item = &AttachmentResolveDiagnostic<R>> {
        self.entries
            .iter()
            .filter(|entry| entry.last_seen_frame == self.current_frame)
    }

    /// Number of retained diagnostic entries.
    #[must_use]
    pub fn len(&self) -> usize { self.entries.len() }

    /// Whether no diagnostic entries are retained.
    #[must_use]
    pub fn is_empty(&self) -> bool { self.entries.is_empty() }
}

impl<R: Send + Sync + 'static> Default for AttachmentResolveDiagnostics<R> {
    fn default() -> Self {
        Self {
            current_frame: 0,
            entries:       VecDeque::new(),
            capacity:      Self::DEFAULT_CAPACITY,
        }
    }
}

/// One diagnostic entry for a skipped attachment edge.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AttachmentResolveDiagnostic<R> {
    /// Entity that could not be resolved.
    pub source:           Entity,
    /// Entity that `source` attempted to target.
    pub target:           Entity,
    /// Consumer-specific skip reason.
    pub reason:           R,
    /// First resolve frame that recorded this source, target, and reason.
    pub first_seen_frame: u64,
    /// Most recent resolve frame that recorded this source, target, and reason.
    pub last_seen_frame:  u64,
    /// Number of times this source, target, and reason has been recorded.
    pub count:            u32,
}

#[cfg(test)]
mod tests {
    use bevy_ecs::entity::Entity;

    use super::AttachmentResolveAction;
    use super::AttachmentResolveCandidate;
    use super::AttachmentResolveDiagnostic;
    use super::AttachmentResolveDiagnostics;
    use super::AttachmentResolveReasons;
    use super::resolve_attachments;
    use crate::AnchorId;
    use crate::AnchoredTo;

    #[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
    enum TestSkip {
        BlockedByCycle,
        BlockedBySkippedDependency,
        Cycle,
        MissingTarget,
        PlacementFailed,
        SourceInvalid,
    }

    fn entity(index: u64) -> Entity { Entity::from_bits(index) }

    fn attachment(target: Entity) -> AnchoredTo {
        AnchoredTo::new(target, AnchorId::Center, AnchorId::Center)
    }

    const fn test_reasons() -> AttachmentResolveReasons<TestSkip> {
        AttachmentResolveReasons {
            blocked_by_skipped_dependency: TestSkip::BlockedBySkippedDependency,
            cycle:                         TestSkip::Cycle,
            blocked_by_cycle:              TestSkip::BlockedByCycle,
        }
    }

    #[test]
    fn chain_resolves_parent_before_child_in_one_pass() {
        let root = entity(1);
        let middle = entity(2);
        let leaf = entity(3);
        let mut diagnostics = AttachmentResolveDiagnostics::default();
        let mut actions = Vec::new();

        resolve_attachments(
            vec![
                AttachmentResolveCandidate::Active {
                    source:     leaf,
                    target:     middle,
                    attachment: attachment(middle),
                },
                AttachmentResolveCandidate::Active {
                    source:     middle,
                    target:     root,
                    attachment: attachment(root),
                },
            ],
            test_reasons(),
            &mut diagnostics,
            |action| {
                actions.push(action);
                Ok(())
            },
        );

        assert_eq!(
            actions,
            vec![
                AttachmentResolveAction::Place {
                    source:     middle,
                    target:     root,
                    attachment: attachment(root),
                },
                AttachmentResolveAction::Place {
                    source:     leaf,
                    target:     middle,
                    attachment: attachment(middle),
                },
            ]
        );
        assert!(diagnostics.current().next().is_none());
    }

    #[test]
    fn skipped_candidate_routes_to_fallback_and_records_reason() {
        let source = entity(1);
        let target = entity(2);
        let mut diagnostics = AttachmentResolveDiagnostics::default();
        let mut actions = Vec::new();

        resolve_attachments(
            vec![AttachmentResolveCandidate::Skipped {
                source,
                target,
                reason: TestSkip::SourceInvalid,
            }],
            test_reasons(),
            &mut diagnostics,
            |action| {
                actions.push(action);
                Ok(())
            },
        );

        assert_eq!(actions, vec![AttachmentResolveAction::Fallback { source }]);
        assert_eq!(
            diagnostics.current().copied().collect::<Vec<_>>(),
            vec![AttachmentResolveDiagnostic {
                source,
                target,
                reason: TestSkip::SourceInvalid,
                first_seen_frame: 1,
                last_seen_frame: 1,
                count: 1,
            }]
        );
    }

    #[test]
    fn diagnostics_accumulate_across_frames() {
        let source = entity(1);
        let target = entity(2);
        let mut diagnostics = AttachmentResolveDiagnostics {
            capacity: 2,
            ..Default::default()
        };

        resolve_attachments(
            vec![AttachmentResolveCandidate::Skipped {
                source,
                target,
                reason: TestSkip::PlacementFailed,
            }],
            test_reasons(),
            &mut diagnostics,
            |_| Ok(()),
        );
        resolve_attachments(
            vec![AttachmentResolveCandidate::Skipped {
                source,
                target,
                reason: TestSkip::PlacementFailed,
            }],
            test_reasons(),
            &mut diagnostics,
            |_| Ok(()),
        );
        resolve_attachments(
            Vec::<AttachmentResolveCandidate<TestSkip>>::new(),
            test_reasons(),
            &mut diagnostics,
            |_| Ok(()),
        );

        assert_eq!(
            diagnostics.entries().copied().collect::<Vec<_>>(),
            vec![AttachmentResolveDiagnostic {
                source,
                target,
                reason: TestSkip::PlacementFailed,
                first_seen_frame: 1,
                last_seen_frame: 2,
                count: 2,
            }]
        );
        assert!(diagnostics.current().next().is_none());
    }

    #[test]
    fn missing_target_skip_uses_fallback_path() {
        let source = entity(1);
        let target = entity(2);
        let mut diagnostics = AttachmentResolveDiagnostics::default();
        let mut actions = Vec::new();

        resolve_attachments(
            vec![AttachmentResolveCandidate::Skipped {
                source,
                target,
                reason: TestSkip::MissingTarget,
            }],
            test_reasons(),
            &mut diagnostics,
            |action| {
                actions.push(action);
                Ok(())
            },
        );

        assert_eq!(actions, vec![AttachmentResolveAction::Fallback { source }]);
        assert_eq!(
            diagnostics.current().copied().collect::<Vec<_>>(),
            vec![AttachmentResolveDiagnostic {
                source,
                target,
                reason: TestSkip::MissingTarget,
                first_seen_frame: 1,
                last_seen_frame: 1,
                count: 1,
            }]
        );
    }
}
