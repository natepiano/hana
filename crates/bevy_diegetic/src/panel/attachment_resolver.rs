//! Shared dependency resolution for panel attachments.

use std::collections::VecDeque;
use std::hash::Hash;

use bevy::platform::collections::HashMap;
use bevy::platform::collections::HashSet;
use bevy::prelude::*;

use super::AnchoredToPanel;

/// Attachment edge after coordinate-space-specific validation.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum AttachmentResolveCandidate<R> {
    /// Edge can be resolved by the caller's coordinate-space adapter.
    Active {
        source:     Entity,
        target:     Entity,
        attachment: AnchoredToPanel,
    },
    /// Edge is owned by this resolver but invalid for this frame.
    Skipped {
        source: Entity,
        target: Entity,
        reason: R,
    },
}

/// Shared skip reasons used by the dependency resolver.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct AttachmentResolveReasons<R> {
    pub(crate) blocked_by_skipped_dependency: R,
    pub(crate) cycle:                         R,
    pub(crate) blocked_by_cycle:              R,
}

/// Coordinate-space-specific work requested by the shared resolver.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum AttachmentResolveAction {
    Place {
        source:     Entity,
        target:     Entity,
        attachment: AnchoredToPanel,
    },
    Fallback {
        source: Entity,
    },
}

/// Resolves active candidates in dependency order and reports skipped edges.
pub(crate) fn resolve_panel_attachments<R, F>(
    candidates: Vec<AttachmentResolveCandidate<R>>,
    reasons: AttachmentResolveReasons<R>,
    diagnostics: &mut AttachmentResolveDiagnostics<R>,
    mut handle: F,
) where
    R: Copy + Eq + Hash + Send + Sync + 'static,
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

#[derive(Default)]
struct AttachmentGraph {
    adjacency:   HashMap<Entity, Vec<Entity>>,
    attachments: HashMap<Entity, AnchoredToPanel>,
    indegree:    HashMap<Entity, usize>,
    target_of:   HashMap<Entity, Entity>,
}

impl AttachmentGraph {
    fn add(&mut self, source: Entity, target: Entity, attachment: AnchoredToPanel) {
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
        R: Copy + Eq + Hash + Send + Sync + 'static,
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
        R: Copy + Eq + Hash + Send + Sync + 'static,
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
        R: Copy + Eq + Hash + Send + Sync + 'static,
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
        R: Copy + Eq + Hash + Send + Sync + 'static,
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
pub(crate) struct AttachmentResolveDiagnostics<R: Send + Sync + 'static> {
    current_frame: u64,
    entries:       VecDeque<AttachmentResolveDiagnostic<R>>,
    capacity:      usize,
}

impl<R: Send + Sync + 'static> AttachmentResolveDiagnostics<R> {
    pub(crate) const DEFAULT_CAPACITY: usize = 128;

    const fn begin_frame(&mut self) { self.current_frame += 1; }

    fn record(&mut self, source: Entity, target: Entity, reason: R)
    where
        R: Copy + Eq,
    {
        if let Some(entry) = self.entries.iter_mut().find(|entry| {
            entry.source == source && entry.target == target && entry.reason == reason
        }) {
            entry.last_seen_frame = self.current_frame;
            entry.count += 1;
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

    #[cfg(test)]
    pub(crate) fn current(&self) -> impl Iterator<Item = &AttachmentResolveDiagnostic<R>> {
        self.entries
            .iter()
            .filter(|entry| entry.last_seen_frame == self.current_frame)
    }
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
pub(crate) struct AttachmentResolveDiagnostic<R> {
    pub(crate) source:           Entity,
    pub(crate) target:           Entity,
    pub(crate) reason:           R,
    pub(crate) first_seen_frame: u64,
    pub(crate) last_seen_frame:  u64,
    pub(crate) count:            u32,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
    enum TestSkip {
        BadUnits,
        MissingTarget,
        CrossWindow,
    }

    #[test]
    fn diagnostics_coalesce_recover_store_reasons_and_evict() {
        let source = Entity::from_bits(1);
        let target = Entity::from_bits(2);
        let other = Entity::from_bits(3);
        let mut diagnostics = AttachmentResolveDiagnostics {
            capacity: 2,
            ..Default::default()
        };

        diagnostics.begin_frame();
        diagnostics.record(source, target, TestSkip::BadUnits);
        diagnostics.begin_frame();
        diagnostics.record(source, target, TestSkip::BadUnits);

        assert_eq!(diagnostics.entries.len(), 1);
        assert_eq!(diagnostics.entries[0].count, 2);
        assert_eq!(diagnostics.entries[0].first_seen_frame, 1);
        assert_eq!(diagnostics.entries[0].last_seen_frame, 2);

        diagnostics.begin_frame();
        assert!(
            diagnostics.current().next().is_none(),
            "recovered edge should stop being current"
        );

        diagnostics.record(source, target, TestSkip::MissingTarget);
        diagnostics.record(other, target, TestSkip::CrossWindow);

        assert_eq!(diagnostics.entries.len(), 2);
        assert_eq!(diagnostics.entries[0].reason, TestSkip::MissingTarget);
        assert_eq!(diagnostics.entries[1].reason, TestSkip::CrossWindow);
    }
}
