//! Screen-space panel attachment resolution.

use std::collections::VecDeque;

use bevy::platform::collections::HashMap;
use bevy::platform::collections::HashSet;
use bevy::prelude::*;
use bevy::window::PrimaryWindow;
use bevy::window::WindowRef;

use crate::layout::Anchor;
use crate::panel::AnchoredToPanel;
use crate::panel::CoordinateSpace;
use crate::panel::DiegeticPanel;
use crate::panel::PanelAnchorOffsetUnits;
use crate::panel::ResolvedScreenPanelPosition;
use crate::panel::ScreenPosition;

const DEFAULT_DIAGNOSTIC_CAPACITY: usize = 128;

/// Resolves screen-space panel attachments for this frame.
pub(super) fn resolve_screen_space_panel_attachments(
    windows: Query<(Entity, &Window)>,
    primary: Query<Entity, With<PrimaryWindow>>,
    entities: Query<()>,
    attachments: Query<(Entity, &AnchoredToPanel)>,
    panels: Query<(Entity, &DiegeticPanel), With<ResolvedScreenPanelPosition>>,
    mut resolved_positions: Query<&mut ResolvedScreenPanelPosition>,
    mut diagnostics: ResMut<AnchorResolveDiagnostics>,
) {
    diagnostics.begin_frame();

    let window_sizes = window_size_lookup(&windows);
    let mut desired_positions = desired_position_map(&panels);
    let mut snapshots = panel_snapshots(&panels, &primary, &window_sizes);
    let candidates = classify_candidates(&attachments, &panels, &entities, &primary, &window_sizes);

    let mut graph = AttachmentGraph::default();
    let mut states = HashMap::default();
    for candidate in candidates {
        match candidate {
            Candidate::Active {
                source,
                target,
                attachment,
            } => {
                if snapshots.contains_key(&source) && snapshots.contains_key(&target) {
                    graph.add(source, target, attachment);
                }
            },
            Candidate::Skipped {
                source,
                target,
                reason,
            } => {
                states.insert(source, AnchorResolveState::Skipped(reason));
                diagnostics.record(source, target, reason);
            },
        }
    }

    graph.resolve(
        &mut snapshots,
        &mut states,
        &mut desired_positions,
        &mut diagnostics,
    );
    graph.mark_unresolved_cycles(&mut states, &mut desired_positions, &mut diagnostics);
    write_desired_positions(desired_positions, &mut resolved_positions);
}

fn desired_position_map(
    panels: &Query<(Entity, &DiegeticPanel), With<ResolvedScreenPanelPosition>>,
) -> HashMap<Entity, Option<Vec2>> {
    let mut desired_positions = HashMap::default();
    for (entity, _) in panels {
        desired_positions.insert(entity, None);
    }
    desired_positions
}

fn panel_snapshots(
    panels: &Query<(Entity, &DiegeticPanel), With<ResolvedScreenPanelPosition>>,
    primary: &Query<Entity, With<PrimaryWindow>>,
    window_sizes: &HashMap<Entity, Vec2>,
) -> HashMap<Entity, ScreenPanelSnapshot> {
    let mut snapshots = HashMap::default();
    for (entity, panel) in panels {
        let CoordinateSpace::Screen { window, .. } = panel.coordinate_space() else {
            continue;
        };
        let Ok((_, window_size)) = resolve_window(*window, primary, window_sizes) else {
            continue;
        };
        let snapshot = ScreenPanelSnapshot::from_panel(panel, window_size);
        snapshots.insert(entity, snapshot);
    }
    snapshots
}

fn write_desired_positions(
    desired_positions: HashMap<Entity, Option<Vec2>>,
    resolved_positions: &mut Query<&mut ResolvedScreenPanelPosition>,
) {
    for (entity, anchor_position) in desired_positions {
        let Ok(mut resolved_position) = resolved_positions.get_mut(entity) else {
            continue;
        };
        if resolved_position.anchor_position != anchor_position {
            resolved_position.anchor_position = anchor_position;
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Candidate {
    Active {
        source:     Entity,
        target:     Entity,
        attachment: AnchoredToPanel,
    },
    Skipped {
        source: Entity,
        target: Entity,
        reason: AnchorResolveSkip,
    },
}

fn classify_candidates(
    attachments: &Query<(Entity, &AnchoredToPanel)>,
    panels: &Query<(Entity, &DiegeticPanel), With<ResolvedScreenPanelPosition>>,
    entities: &Query<()>,
    primary: &Query<Entity, With<PrimaryWindow>>,
    window_sizes: &HashMap<Entity, Vec2>,
) -> Vec<Candidate> {
    let mut candidates = Vec::new();
    for (source, attachment) in attachments {
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
    attachment: AnchoredToPanel,
    panels: &Query<(Entity, &DiegeticPanel), With<ResolvedScreenPanelPosition>>,
    entities: &Query<()>,
    primary: &Query<Entity, With<PrimaryWindow>>,
    window_sizes: &HashMap<Entity, Vec2>,
) -> Candidate {
    let target = attachment.target();
    match validate_candidate(source, attachment, panels, entities, primary, window_sizes) {
        Ok(()) => Candidate::Active {
            source,
            target,
            attachment,
        },
        Err(reason) => Candidate::Skipped {
            source,
            target,
            reason,
        },
    }
}

fn validate_candidate(
    source: Entity,
    attachment: AnchoredToPanel,
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
    if attachment.offset.units() != PanelAnchorOffsetUnits::ScreenPixels {
        return Err(AnchorResolveSkip::OffsetUnitsMismatch);
    }

    let source_window =
        resolve_window(*source_window, primary, window_sizes).map_err(|failure| match failure {
            WindowResolveFailure::Missing => AnchorResolveSkip::SourceWindowMissing,
            WindowResolveFailure::ZeroSized => AnchorResolveSkip::SourceWindowZeroSized,
        })?;
    let target_window =
        resolve_window(*target_window, primary, window_sizes).map_err(|failure| match failure {
            WindowResolveFailure::Missing => AnchorResolveSkip::TargetWindowMissing,
            WindowResolveFailure::ZeroSized => AnchorResolveSkip::TargetWindowZeroSized,
        })?;
    if source_window != target_window {
        return Err(AnchorResolveSkip::CrossWindow);
    }

    Ok(())
}

#[derive(Clone, Copy, Debug)]
struct ScreenPanelSnapshot {
    anchor_position: Vec2,
    anchor:          Anchor,
    size:            Vec2,
}

impl ScreenPanelSnapshot {
    fn from_panel(panel: &DiegeticPanel, window_size: Vec2) -> Self {
        Self {
            anchor_position: configured_anchor_position(panel, window_size),
            anchor:          panel.anchor(),
            size:            Vec2::new(panel.width(), panel.height()),
        }
    }

    fn bounds(self) -> ScreenPanelBounds {
        let anchor_offset = anchor_offset(self.anchor, self.size);
        ScreenPanelBounds {
            top_left: self.anchor_position - anchor_offset,
            size:     self.size,
        }
    }
}

#[derive(Clone, Copy, Debug)]
struct ScreenPanelBounds {
    top_left: Vec2,
    size:     Vec2,
}

impl ScreenPanelBounds {
    fn point(self, anchor: Anchor) -> Vec2 { self.top_left + anchor_offset(anchor, self.size) }
}

fn configured_anchor_position(panel: &DiegeticPanel, window_size: Vec2) -> Vec2 {
    let CoordinateSpace::Screen { position, .. } = panel.coordinate_space() else {
        return Vec2::ZERO;
    };
    match *position {
        ScreenPosition::Screen => {
            let (x, y) = panel.anchor().offset_fraction();
            Vec2::new(x * window_size.x, y * window_size.y)
        },
        ScreenPosition::At(position) => position,
    }
}

fn anchor_offset(anchor: Anchor, size: Vec2) -> Vec2 {
    let (x, y) = anchor.offset(size.x, size.y);
    Vec2::new(x, y)
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

    fn resolve(
        &mut self,
        snapshots: &mut HashMap<Entity, ScreenPanelSnapshot>,
        states: &mut HashMap<Entity, AnchorResolveState>,
        desired_positions: &mut HashMap<Entity, Option<Vec2>>,
        diagnostics: &mut AnchorResolveDiagnostics,
    ) {
        for entity in snapshots.keys().copied() {
            self.indegree.entry(entity).or_default();
        }

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
                .unwrap_or(AnchorResolveState::Configured);
            self.resolve_children(
                entity,
                state,
                snapshots,
                states,
                desired_positions,
                diagnostics,
                &mut queue,
            );
        }
    }

    fn resolve_children(
        &mut self,
        target: Entity,
        target_state: AnchorResolveState,
        snapshots: &mut HashMap<Entity, ScreenPanelSnapshot>,
        states: &mut HashMap<Entity, AnchorResolveState>,
        desired_positions: &mut HashMap<Entity, Option<Vec2>>,
        diagnostics: &mut AnchorResolveDiagnostics,
        queue: &mut VecDeque<Entity>,
    ) {
        let children = self.adjacency.get(&target).cloned().unwrap_or_default();
        for child in children {
            match target_state {
                AnchorResolveState::Skipped(_) => {
                    states.insert(
                        child,
                        AnchorResolveState::Skipped(AnchorResolveSkip::BlockedBySkippedDependency),
                    );
                    desired_positions.insert(child, None);
                    diagnostics.record(
                        child,
                        target,
                        AnchorResolveSkip::BlockedBySkippedDependency,
                    );
                },
                AnchorResolveState::Configured | AnchorResolveState::Resolved => {
                    self.resolve_child_position(
                        child,
                        target,
                        snapshots,
                        states,
                        desired_positions,
                    );
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

    fn resolve_child_position(
        &self,
        child: Entity,
        target: Entity,
        snapshots: &mut HashMap<Entity, ScreenPanelSnapshot>,
        states: &mut HashMap<Entity, AnchorResolveState>,
        desired_positions: &mut HashMap<Entity, Option<Vec2>>,
    ) {
        let Some(target_snapshot) = snapshots.get(&target).copied() else {
            return;
        };
        let Some(child_snapshot) = snapshots.get(&child).copied() else {
            return;
        };
        let Some(attachment) = self.attachments.get(&child).copied() else {
            return;
        };

        let target_point =
            target_snapshot.bounds().point(attachment.target_anchor) + attachment.offset.as_vec2();
        let source_offset = anchor_offset(attachment.source_anchor, child_snapshot.size);
        let panel_offset = anchor_offset(child_snapshot.anchor, child_snapshot.size);
        let top_left = target_point - source_offset;
        let anchor_position = top_left + panel_offset;

        desired_positions.insert(child, Some(anchor_position));
        snapshots.insert(
            child,
            ScreenPanelSnapshot {
                anchor_position,
                ..child_snapshot
            },
        );
        states.insert(child, AnchorResolveState::Resolved);
    }

    fn mark_unresolved_cycles(
        &self,
        states: &mut HashMap<Entity, AnchorResolveState>,
        desired_positions: &mut HashMap<Entity, Option<Vec2>>,
        diagnostics: &mut AnchorResolveDiagnostics,
    ) {
        let remaining = self.remaining_nodes();
        let cycle_members = cycle_members(&remaining, &self.target_of);
        for entity in remaining {
            let reason = if cycle_members.contains(&entity) {
                AnchorResolveSkip::Cycle
            } else {
                AnchorResolveSkip::BlockedByCycle
            };
            let target = self.target_of.get(&entity).copied().unwrap_or(entity);
            states.insert(entity, AnchorResolveState::Skipped(reason));
            desired_positions.insert(entity, None);
            diagnostics.record(entity, target, reason);
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
enum AnchorResolveState {
    Configured,
    Resolved,
    Skipped(AnchorResolveSkip),
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
    OffsetUnitsMismatch,
    Cycle,
    BlockedByCycle,
    BlockedBySkippedDependency,
    UnsupportedWorldParentTransform,
}

/// Bounded history of screen attachment resolution failures.
#[derive(Resource, Debug)]
pub(crate) struct AnchorResolveDiagnostics {
    current_frame: u64,
    entries:       VecDeque<AnchorResolveDiagnostic>,
    capacity:      usize,
}

impl Default for AnchorResolveDiagnostics {
    fn default() -> Self {
        Self {
            current_frame: 0,
            entries:       VecDeque::new(),
            capacity:      DEFAULT_DIAGNOSTIC_CAPACITY,
        }
    }
}

impl AnchorResolveDiagnostics {
    const fn begin_frame(&mut self) { self.current_frame += 1; }

    fn record(&mut self, source: Entity, target: Entity, reason: AnchorResolveSkip) {
        if let Some(entry) = self.entries.iter_mut().find(|entry| {
            entry.source == source && entry.target == target && entry.reason == reason
        }) {
            entry.last_seen_frame = self.current_frame;
            entry.count += 1;
            return;
        }

        self.entries.push_back(AnchorResolveDiagnostic {
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
    pub(crate) fn current(&self) -> impl Iterator<Item = &AnchorResolveDiagnostic> {
        self.entries
            .iter()
            .filter(|entry| entry.last_seen_frame == self.current_frame)
    }
}

/// One diagnostic entry for a skipped attachment edge.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct AnchorResolveDiagnostic {
    pub(crate) source:           Entity,
    pub(crate) target:           Entity,
    pub(crate) reason:           AnchorResolveSkip,
    pub(crate) first_seen_frame: u64,
    pub(crate) last_seen_frame:  u64,
    pub(crate) count:            u32,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum WindowResolveFailure {
    Missing,
    ZeroSized,
}

fn resolve_window(
    window_ref: WindowRef,
    primary: &Query<Entity, With<PrimaryWindow>>,
    window_sizes: &HashMap<Entity, Vec2>,
) -> Result<(Entity, Vec2), WindowResolveFailure> {
    let window = match window_ref {
        WindowRef::Primary => primary
            .single()
            .map_err(|_| WindowResolveFailure::Missing)?,
        WindowRef::Entity(entity) => entity,
    };
    let Some(size) = window_sizes.get(&window).copied() else {
        return Err(WindowResolveFailure::Missing);
    };
    if size.x <= 0.0 || size.y <= 0.0 {
        return Err(WindowResolveFailure::ZeroSized);
    }
    Ok((window, size))
}

fn window_size_lookup(windows: &Query<(Entity, &Window)>) -> HashMap<Entity, Vec2> {
    let mut window_sizes = HashMap::default();
    for (entity, window) in windows {
        window_sizes.insert(entity, Vec2::new(window.width(), window.height()));
    }
    window_sizes
}

#[cfg(test)]
#[allow(
    clippy::expect_used,
    reason = "tests should panic on unexpected values"
)]
mod tests {
    use std::sync::Arc;

    use bevy::prelude::*;
    use bevy::window::PrimaryWindow;
    use bevy::window::Window;

    use super::AnchorResolveDiagnostics;
    use super::AnchorResolveSkip;
    use crate::AnchoredToPanel;
    use crate::Fit;
    use crate::HeadlessLayoutPlugin;
    use crate::PanelAnchorOffset;
    use crate::PanelDimensionsChanged;
    use crate::PanelSystems;
    use crate::Px;
    use crate::ScreenPosition;
    use crate::layout::Anchor;
    use crate::layout::TextDimensions;
    use crate::layout::TextMeasure;
    use crate::layout::TextStyle;
    use crate::panel::DiegeticPanel;
    use crate::panel::ResolvedScreenPanelPosition;
    use crate::screen_space::ScreenSpacePlugin;
    use crate::screen_space::ScreenSpaceSystems;
    use crate::text::DiegeticTextMeasurer;

    #[derive(Component)]
    struct SourcePanel;

    #[derive(Component)]
    struct DependentPanel;

    #[derive(Resource)]
    struct AttachmentWrite {
        source: Entity,
        target: Entity,
        done:   bool,
    }

    #[derive(Resource, Default)]
    struct ResolverChangeLog(Vec<Vec<(Entity, Option<Vec2>)>>);

    fn app_with_window() -> App {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        app.insert_resource(DiegeticTextMeasurer {
            measure_fn: Arc::new(|_text: &str, measure: &TextMeasure| TextDimensions {
                width:       measure.size,
                height:      measure.size,
                line_height: measure.size,
            }),
        });
        app.add_plugins(HeadlessLayoutPlugin);
        app.add_plugins(ScreenSpacePlugin);
        app.world_mut().spawn((
            Window {
                resolution: (800_u32, 600_u32).into(),
                ..Default::default()
            },
            PrimaryWindow,
        ));
        app
    }

    fn fixed_screen_panel(size: Vec2, anchor: Anchor, screen_position: Vec2) -> DiegeticPanel {
        DiegeticPanel::screen()
            .size(Px(size.x), Px(size.y))
            .anchor(anchor)
            .screen_position(screen_position.x, screen_position.y)
            .layout(|_| {})
            .build()
            .expect("screen panel builds")
    }

    fn fixed_screen_panel_in_window(
        window: Entity,
        size: Vec2,
        anchor: Anchor,
        screen_position: Vec2,
    ) -> DiegeticPanel {
        DiegeticPanel::screen()
            .size(Px(size.x), Px(size.y))
            .anchor(anchor)
            .screen_position(screen_position.x, screen_position.y)
            .window_entity(window)
            .layout(|_| {})
            .build()
            .expect("screen panel builds")
    }

    fn fit_screen_panel(anchor: Anchor, screen_position: Vec2) -> DiegeticPanel {
        DiegeticPanel::screen()
            .size(Fit, Fit)
            .anchor(anchor)
            .screen_position(screen_position.x, screen_position.y)
            .layout(|builder| {
                builder.text("fit", TextStyle::new(10.0));
            })
            .build()
            .expect("fit screen panel builds")
    }

    fn assert_close(actual: f32, expected: f32) {
        assert!(
            (actual - expected).abs() < 1e-4,
            "expected {expected}, got {actual}",
        );
    }

    fn assert_translation(app: &App, entity: Entity, expected: Vec2) {
        let transform = app
            .world()
            .get::<Transform>(entity)
            .expect("panel has transform");
        assert_close(transform.translation.x, expected.x);
        assert_close(transform.translation.y, expected.y);
    }

    fn assert_current_diagnostic(
        app: &App,
        source: Entity,
        target: Entity,
        reason: AnchorResolveSkip,
    ) {
        let diagnostics = app.world().resource::<AnchorResolveDiagnostics>();
        assert!(
            diagnostics.current().any(|entry| entry.source == source
                && entry.target == target
                && entry.reason == reason),
            "missing current diagnostic {reason:?}",
        );
    }

    fn resolved_anchor_position(app: &App, entity: Entity) -> Option<Vec2> {
        app.world()
            .get::<ResolvedScreenPanelPosition>(entity)
            .expect("panel has resolved position")
            .anchor_position
    }

    fn record_non_added_resolver_changes(
        panels: Query<(Entity, Ref<ResolvedScreenPanelPosition>)>,
        mut log: ResMut<ResolverChangeLog>,
    ) {
        let mut frame_changes = Vec::new();
        for (entity, resolved) in &panels {
            if resolved.is_changed() && !resolved.is_added() {
                frame_changes.push((entity, resolved.anchor_position));
            }
        }
        log.0.push(frame_changes);
    }

    #[test]
    fn screen_attachment_positions_source_anchor_against_target_anchor() {
        let mut app = app_with_window();
        let target = app
            .world_mut()
            .spawn(fixed_screen_panel(
                Vec2::new(200.0, 40.0),
                Anchor::TopLeft,
                Vec2::new(100.0, 100.0),
            ))
            .id();
        let source = app
            .world_mut()
            .spawn((
                fixed_screen_panel(Vec2::new(50.0, 10.0), Anchor::TopLeft, Vec2::new(0.0, 0.0)),
                AnchoredToPanel::new(target, Anchor::TopLeft, Anchor::BottomLeft)
                    .with_screen_offset(Vec2::new(0.0, 1.0)),
            ))
            .id();

        app.update();

        let resolved = app
            .world()
            .get::<ResolvedScreenPanelPosition>(source)
            .expect("source has resolved position");
        assert_eq!(resolved.anchor_position, Some(Vec2::new(100.0, 141.0)));
        assert_translation(&app, source, Vec2::new(-300.0, 159.0));
    }

    #[test]
    fn attachment_math_handles_different_panel_and_source_anchors() {
        let mut app = app_with_window();
        let target = app
            .world_mut()
            .spawn(fixed_screen_panel(
                Vec2::new(200.0, 40.0),
                Anchor::TopLeft,
                Vec2::new(100.0, 100.0),
            ))
            .id();
        let source = app
            .world_mut()
            .spawn((
                fixed_screen_panel(Vec2::new(50.0, 10.0), Anchor::Center, Vec2::new(0.0, 0.0)),
                AnchoredToPanel::new(target, Anchor::BottomRight, Anchor::TopRight)
                    .with_screen_offset(Vec2::new(5.0, 2.0)),
            ))
            .id();

        app.update();

        assert_eq!(
            resolved_anchor_position(&app, source),
            Some(Vec2::new(280.0, 97.0))
        );
        assert_translation(&app, source, Vec2::new(-120.0, 203.0));
    }

    #[test]
    fn first_frame_fit_target_resolves_before_attachment_positioning() {
        let mut app = app_with_window();
        let target = app
            .world_mut()
            .spawn(fit_screen_panel(Anchor::TopLeft, Vec2::new(100.0, 100.0)))
            .id();
        let source = app
            .world_mut()
            .spawn((
                fixed_screen_panel(Vec2::new(50.0, 10.0), Anchor::TopLeft, Vec2::new(0.0, 0.0)),
                AnchoredToPanel::new(target, Anchor::TopLeft, Anchor::BottomLeft)
                    .with_screen_offset(Vec2::new(0.0, 1.0)),
            ))
            .id();

        app.update();

        let target_panel = app
            .world()
            .get::<DiegeticPanel>(target)
            .expect("target still exists");
        assert!(target_panel.width() > 0.0);
        assert!(target_panel.height() > 0.0);
        assert_translation(
            &app,
            source,
            Vec2::new(-300.0, 300.0 - 100.0 - target_panel.height() - 1.0),
        );
    }

    fn attach_dependent_from_dimension_event(
        event: On<PanelDimensionsChanged>,
        sources: Query<(), With<SourcePanel>>,
        dependents: Query<Entity, With<DependentPanel>>,
        mut commands: Commands,
    ) {
        if sources.get(event.event().entity).is_err() {
            return;
        }
        for dependent in &dependents {
            commands.entity(dependent).insert(
                AnchoredToPanel::new(event.event().entity, Anchor::TopLeft, Anchor::BottomLeft)
                    .with_screen_offset(Vec2::new(0.0, 1.0)),
            );
        }
    }

    #[test]
    fn dimension_observer_can_queue_attachment_for_same_update_positioning() {
        let mut app = app_with_window();
        app.add_observer(attach_dependent_from_dimension_event);
        let target = app
            .world_mut()
            .spawn((
                SourcePanel,
                fixed_screen_panel(
                    Vec2::new(200.0, 40.0),
                    Anchor::TopLeft,
                    Vec2::new(100.0, 100.0),
                ),
            ))
            .id();
        let dependent = app
            .world_mut()
            .spawn((
                DependentPanel,
                fixed_screen_panel(Vec2::new(50.0, 10.0), Anchor::TopLeft, Vec2::new(0.0, 0.0)),
            ))
            .id();

        app.update();

        let attachment = app
            .world()
            .get::<AnchoredToPanel>(dependent)
            .expect("observer inserted relationship");
        assert_eq!(attachment.target(), target);
        assert_translation(&app, dependent, Vec2::new(-300.0, 159.0));
    }

    fn queue_attachment_once(mut commands: Commands, mut write: ResMut<AttachmentWrite>) {
        if write.done {
            return;
        }
        commands.entity(write.source).insert(AnchoredToPanel::new(
            write.target,
            Anchor::TopLeft,
            Anchor::BottomLeft,
        ));
        write.done = true;
    }

    #[test]
    fn command_writes_before_observer_flush_affect_current_update() {
        let mut app = app_with_window();
        let target = app
            .world_mut()
            .spawn(fixed_screen_panel(
                Vec2::new(200.0, 40.0),
                Anchor::TopLeft,
                Vec2::new(100.0, 100.0),
            ))
            .id();
        let source = app
            .world_mut()
            .spawn(fixed_screen_panel(
                Vec2::new(50.0, 10.0),
                Anchor::TopLeft,
                Vec2::new(0.0, 0.0),
            ))
            .id();
        app.insert_resource(AttachmentWrite {
            source,
            target,
            done: false,
        });
        app.add_systems(
            Update,
            queue_attachment_once.before(ScreenSpaceSystems::FlushObserverCommands),
        );

        app.update();

        assert_translation(&app, source, Vec2::new(-300.0, 160.0));
    }

    #[test]
    fn command_writes_after_resolver_affect_next_update() {
        let mut app = app_with_window();
        let target = app
            .world_mut()
            .spawn(fixed_screen_panel(
                Vec2::new(200.0, 40.0),
                Anchor::TopLeft,
                Vec2::new(100.0, 100.0),
            ))
            .id();
        let source = app
            .world_mut()
            .spawn(fixed_screen_panel(
                Vec2::new(50.0, 10.0),
                Anchor::TopLeft,
                Vec2::new(0.0, 0.0),
            ))
            .id();
        app.insert_resource(AttachmentWrite {
            source,
            target,
            done: false,
        });
        app.add_systems(
            Update,
            queue_attachment_once.after(PanelSystems::ResolvePanelAttachments),
        );

        app.update();

        assert_translation(&app, source, Vec2::new(-400.0, 300.0));

        app.update();

        assert_translation(&app, source, Vec2::new(-300.0, 160.0));
    }

    #[test]
    fn target_size_and_position_changes_reposition_dependent() {
        let mut app = app_with_window();
        let target = app
            .world_mut()
            .spawn(fixed_screen_panel(
                Vec2::new(200.0, 40.0),
                Anchor::TopLeft,
                Vec2::new(100.0, 100.0),
            ))
            .id();
        let source = app
            .world_mut()
            .spawn((
                fixed_screen_panel(Vec2::new(50.0, 10.0), Anchor::TopLeft, Vec2::new(0.0, 0.0)),
                AnchoredToPanel::new(target, Anchor::TopLeft, Anchor::BottomRight),
            ))
            .id();

        app.update();
        assert_translation(&app, source, Vec2::new(-100.0, 160.0));

        app.world_mut()
            .entity_mut(target)
            .insert(fixed_screen_panel(
                Vec2::new(300.0, 50.0),
                Anchor::TopLeft,
                Vec2::new(100.0, 100.0),
            ));
        app.update();
        assert_translation(&app, source, Vec2::new(0.0, 150.0));

        {
            let mut target_panel = app
                .world_mut()
                .get_mut::<DiegeticPanel>(target)
                .expect("target still exists");
            assert!(target_panel.set_screen_position(Vec2::new(120.0, 130.0)));
        }
        app.update();
        assert_translation(&app, source, Vec2::new(20.0, 120.0));
    }

    #[test]
    fn screen_position_screen_targets_track_window_resize() {
        let mut app = app_with_window();
        let window = app
            .world_mut()
            .query_filtered::<Entity, With<PrimaryWindow>>()
            .single(app.world())
            .expect("primary window exists");
        let target = app
            .world_mut()
            .spawn(
                DiegeticPanel::screen()
                    .size(Px(200.0), Px(40.0))
                    .anchor(Anchor::BottomRight)
                    .layout(|_| {})
                    .build()
                    .expect("screen panel builds"),
            )
            .id();
        let source = app
            .world_mut()
            .spawn((
                fixed_screen_panel(Vec2::new(50.0, 10.0), Anchor::TopLeft, Vec2::new(0.0, 0.0)),
                AnchoredToPanel::new(target, Anchor::TopLeft, Anchor::TopLeft),
            ))
            .id();

        app.update();
        assert_translation(&app, source, Vec2::new(200.0, -260.0));

        app.world_mut()
            .get_mut::<Window>(window)
            .expect("window exists")
            .resolution
            .set(1000.0, 700.0);
        app.update();

        assert_translation(&app, source, Vec2::new(300.0, -310.0));
    }

    #[test]
    fn primary_and_entity_window_refs_match_same_window() {
        let mut app = app_with_window();
        let window = app
            .world_mut()
            .query_filtered::<Entity, With<PrimaryWindow>>()
            .single(app.world())
            .expect("primary window exists");
        let target = app
            .world_mut()
            .spawn(fixed_screen_panel(
                Vec2::new(200.0, 40.0),
                Anchor::TopLeft,
                Vec2::new(100.0, 100.0),
            ))
            .id();
        let source = app
            .world_mut()
            .spawn((
                fixed_screen_panel_in_window(
                    window,
                    Vec2::new(50.0, 10.0),
                    Anchor::TopLeft,
                    Vec2::new(0.0, 0.0),
                ),
                AnchoredToPanel::new(target, Anchor::TopLeft, Anchor::BottomLeft),
            ))
            .id();

        app.update();

        assert_translation(&app, source, Vec2::new(-300.0, 160.0));
    }

    #[test]
    fn invalid_offset_units_clear_override_and_emit_diagnostic() {
        let mut app = app_with_window();
        let target = app
            .world_mut()
            .spawn(fixed_screen_panel(
                Vec2::new(200.0, 40.0),
                Anchor::TopLeft,
                Vec2::new(100.0, 100.0),
            ))
            .id();
        let source = app
            .world_mut()
            .spawn((
                fixed_screen_panel(Vec2::new(50.0, 10.0), Anchor::TopLeft, Vec2::new(0.0, 0.0)),
                AnchoredToPanel::new(target, Anchor::TopLeft, Anchor::BottomLeft),
            ))
            .id();

        app.update();
        assert_translation(&app, source, Vec2::new(-300.0, 160.0));

        app.world_mut().entity_mut(source).insert(
            AnchoredToPanel::new(target, Anchor::TopLeft, Anchor::BottomLeft)
                .with_offset(PanelAnchorOffset::target_plane_meters(Vec2::ZERO)),
        );
        app.update();

        let resolved = app
            .world()
            .get::<ResolvedScreenPanelPosition>(source)
            .expect("source has resolved position");
        assert_eq!(resolved.anchor_position, None);
        assert_translation(&app, source, Vec2::new(-400.0, 300.0));
        assert_current_diagnostic(&app, source, target, AnchorResolveSkip::OffsetUnitsMismatch);
    }

    #[test]
    fn removing_attachment_restores_configured_position() {
        let mut app = app_with_window();
        let target = app
            .world_mut()
            .spawn(fixed_screen_panel(
                Vec2::new(200.0, 40.0),
                Anchor::TopLeft,
                Vec2::new(100.0, 100.0),
            ))
            .id();
        let source = app
            .world_mut()
            .spawn((
                fixed_screen_panel(
                    Vec2::new(50.0, 10.0),
                    Anchor::TopLeft,
                    Vec2::new(20.0, 30.0),
                ),
                AnchoredToPanel::new(target, Anchor::TopLeft, Anchor::BottomLeft),
            ))
            .id();

        app.update();
        assert_translation(&app, source, Vec2::new(-300.0, 160.0));

        app.world_mut()
            .entity_mut(source)
            .remove::<AnchoredToPanel>();
        app.update();

        assert_eq!(resolved_anchor_position(&app, source), None);
        assert_translation(&app, source, Vec2::new(-380.0, 270.0));
    }

    #[test]
    fn source_coordinate_space_transition_clears_screen_override() {
        let mut app = app_with_window();
        let target = app
            .world_mut()
            .spawn(fixed_screen_panel(
                Vec2::new(200.0, 40.0),
                Anchor::TopLeft,
                Vec2::new(100.0, 100.0),
            ))
            .id();
        let source = app
            .world_mut()
            .spawn((
                fixed_screen_panel(
                    Vec2::new(50.0, 10.0),
                    Anchor::TopLeft,
                    Vec2::new(20.0, 30.0),
                ),
                AnchoredToPanel::new(target, Anchor::TopLeft, Anchor::BottomLeft),
            ))
            .id();

        app.update();
        assert_translation(&app, source, Vec2::new(-300.0, 160.0));

        let world_panel = DiegeticPanel::world()
            .size(Px(50.0), Px(10.0))
            .world_height(1.0)
            .layout(|_| {})
            .build()
            .expect("world panel builds");
        app.world_mut().entity_mut(source).insert(world_panel);
        app.update();

        assert_eq!(resolved_anchor_position(&app, source), None);
        assert_current_diagnostic(
            &app,
            source,
            target,
            AnchorResolveSkip::MixedCoordinateSpace,
        );
    }

    #[test]
    fn cross_window_attachment_falls_back_with_diagnostic() {
        let mut app = app_with_window();
        let secondary = app
            .world_mut()
            .spawn(Window {
                resolution: (1200_u32, 400_u32).into(),
                ..Default::default()
            })
            .id();
        let target = app
            .world_mut()
            .spawn(fixed_screen_panel(
                Vec2::new(200.0, 40.0),
                Anchor::TopLeft,
                Vec2::new(100.0, 100.0),
            ))
            .id();
        let source = app
            .world_mut()
            .spawn((
                fixed_screen_panel_in_window(
                    secondary,
                    Vec2::new(50.0, 10.0),
                    Anchor::TopLeft,
                    Vec2::new(20.0, 30.0),
                ),
                AnchoredToPanel::new(target, Anchor::TopLeft, Anchor::BottomLeft),
            ))
            .id();

        app.update();

        assert_eq!(resolved_anchor_position(&app, source), None);
        assert_translation(&app, source, Vec2::new(-580.0, 170.0));
        assert_current_diagnostic(&app, source, target, AnchorResolveSkip::CrossWindow);
    }

    #[test]
    fn chain_resolves_and_retargeting_middle_updates_downstream() {
        let mut app = app_with_window();
        let root_a = app
            .world_mut()
            .spawn(fixed_screen_panel(
                Vec2::new(100.0, 20.0),
                Anchor::TopLeft,
                Vec2::new(100.0, 100.0),
            ))
            .id();
        let root_b = app
            .world_mut()
            .spawn(fixed_screen_panel(
                Vec2::new(100.0, 20.0),
                Anchor::TopLeft,
                Vec2::new(200.0, 200.0),
            ))
            .id();
        let middle = app
            .world_mut()
            .spawn((
                fixed_screen_panel(Vec2::new(50.0, 10.0), Anchor::TopLeft, Vec2::ZERO),
                AnchoredToPanel::new(root_a, Anchor::TopLeft, Anchor::BottomLeft),
            ))
            .id();
        let leaf = app
            .world_mut()
            .spawn((
                fixed_screen_panel(Vec2::new(25.0, 10.0), Anchor::TopLeft, Vec2::ZERO),
                AnchoredToPanel::new(middle, Anchor::TopLeft, Anchor::BottomLeft),
            ))
            .id();

        app.update();
        assert_translation(&app, middle, Vec2::new(-300.0, 180.0));
        assert_translation(&app, leaf, Vec2::new(-300.0, 170.0));

        app.world_mut()
            .entity_mut(middle)
            .insert(AnchoredToPanel::new(
                root_b,
                Anchor::TopLeft,
                Anchor::BottomLeft,
            ));
        app.update();

        assert_translation(&app, middle, Vec2::new(-200.0, 80.0));
        assert_translation(&app, leaf, Vec2::new(-200.0, 70.0));
    }

    #[test]
    fn resolver_change_log_ignores_spawn_add_and_stable_frames() {
        let mut app = app_with_window();
        app.init_resource::<ResolverChangeLog>();
        app.add_systems(
            Update,
            record_non_added_resolver_changes.after(PanelSystems::ResolvePanelAttachments),
        );
        let target = app
            .world_mut()
            .spawn(fixed_screen_panel(
                Vec2::new(200.0, 40.0),
                Anchor::TopLeft,
                Vec2::new(100.0, 100.0),
            ))
            .id();
        let source = app
            .world_mut()
            .spawn((
                fixed_screen_panel(Vec2::new(50.0, 10.0), Anchor::TopLeft, Vec2::ZERO),
                AnchoredToPanel::new(target, Anchor::TopLeft, Anchor::BottomLeft),
            ))
            .id();

        app.update();
        app.update();
        app.world_mut()
            .entity_mut(source)
            .remove::<AnchoredToPanel>();
        app.update();
        app.update();

        let log = app.world().resource::<ResolverChangeLog>();
        assert!(
            log.0[0].is_empty(),
            "spawn-frame add is not a resolver change"
        );
        assert!(log.0[1].is_empty(), "stable frame should not change");
        assert_eq!(log.0[2], vec![(source, None)]);
        assert!(log.0[3].is_empty(), "stable fallback should not change");

        let panel = app
            .world()
            .get::<DiegeticPanel>(source)
            .expect("source still exists");
        assert!(matches!(
            panel.coordinate_space(),
            crate::panel::CoordinateSpace::Screen {
                position: ScreenPosition::At(position),
                ..
            } if *position == Vec2::ZERO
        ));
    }

    #[test]
    fn diagnostics_coalesce_recover_store_reasons_and_evict() {
        let source = Entity::from_raw_u32(1).expect("valid entity");
        let target = Entity::from_raw_u32(2).expect("valid entity");
        let other = Entity::from_raw_u32(3).expect("valid entity");
        let mut diagnostics = AnchorResolveDiagnostics {
            capacity: 2,
            ..Default::default()
        };

        diagnostics.begin_frame();
        diagnostics.record(source, target, AnchorResolveSkip::OffsetUnitsMismatch);
        diagnostics.begin_frame();
        diagnostics.record(source, target, AnchorResolveSkip::OffsetUnitsMismatch);

        assert_eq!(diagnostics.entries.len(), 1);
        assert_eq!(diagnostics.entries[0].count, 2);
        assert_eq!(diagnostics.entries[0].first_seen_frame, 1);
        assert_eq!(diagnostics.entries[0].last_seen_frame, 2);

        diagnostics.begin_frame();
        assert!(
            diagnostics.current().next().is_none(),
            "recovered edge should stop being current"
        );

        diagnostics.record(source, target, AnchorResolveSkip::TargetWithoutPanel);
        diagnostics.record(other, target, AnchorResolveSkip::CrossWindow);

        assert_eq!(diagnostics.entries.len(), 2);
        assert_eq!(
            diagnostics.entries[0].reason,
            AnchorResolveSkip::TargetWithoutPanel
        );
        assert_eq!(
            diagnostics.entries[1].reason,
            AnchorResolveSkip::CrossWindow
        );
    }

    #[test]
    fn cycle_does_not_block_unrelated_valid_chain() {
        let mut app = app_with_window();
        let root = app
            .world_mut()
            .spawn(fixed_screen_panel(
                Vec2::new(200.0, 40.0),
                Anchor::TopLeft,
                Vec2::new(100.0, 100.0),
            ))
            .id();
        let valid = app
            .world_mut()
            .spawn((
                fixed_screen_panel(Vec2::new(50.0, 10.0), Anchor::TopLeft, Vec2::new(0.0, 0.0)),
                AnchoredToPanel::new(root, Anchor::TopLeft, Anchor::BottomLeft),
            ))
            .id();
        let cycle_a = app
            .world_mut()
            .spawn(fixed_screen_panel(
                Vec2::new(30.0, 10.0),
                Anchor::TopLeft,
                Vec2::new(20.0, 20.0),
            ))
            .id();
        let cycle_b = app
            .world_mut()
            .spawn((
                fixed_screen_panel(
                    Vec2::new(30.0, 10.0),
                    Anchor::TopLeft,
                    Vec2::new(40.0, 40.0),
                ),
                AnchoredToPanel::new(cycle_a, Anchor::TopLeft, Anchor::BottomLeft),
            ))
            .id();
        app.world_mut()
            .entity_mut(cycle_a)
            .insert(AnchoredToPanel::new(
                cycle_b,
                Anchor::TopLeft,
                Anchor::BottomLeft,
            ));

        app.update();

        assert_translation(&app, valid, Vec2::new(-300.0, 160.0));
        assert_translation(&app, cycle_a, Vec2::new(-380.0, 280.0));
        assert_translation(&app, cycle_b, Vec2::new(-360.0, 260.0));
        assert_current_diagnostic(&app, cycle_a, cycle_b, AnchorResolveSkip::Cycle);
        assert_current_diagnostic(&app, cycle_b, cycle_a, AnchorResolveSkip::Cycle);
    }
}
