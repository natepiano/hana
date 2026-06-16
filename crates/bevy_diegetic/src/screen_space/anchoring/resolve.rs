//! Screen-space panel attachment resolution.

use bevy::prelude::*;
use bevy::window::PrimaryWindow;

use super::AnchorResolveSkip;
use super::candidate;
use super::placement;
use super::placement::ScreenAttachmentPlacer;
use super::rect;
use super::window;
use crate::panel;
use crate::panel::AnchoredToPanel;
use crate::panel::AttachmentResolveCandidate;
use crate::panel::AttachmentResolveDiagnostics;
use crate::panel::DiegeticPanel;
use crate::panel::PanelAnchorPose;
use crate::panel::ResolvedScreenPanelPosition;

pub(crate) type AnchorResolveDiagnostics = AttachmentResolveDiagnostics<AnchorResolveSkip>;

/// Resolves screen-space panel attachments for this frame.
pub(crate) fn resolve_screen_space_panel_attachments(
    windows: Query<(Entity, &Window)>,
    primary: Query<Entity, With<PrimaryWindow>>,
    entities: Query<()>,
    attachments: Query<(Entity, &AnchoredToPanel)>,
    anchor_poses: Query<(Entity, &PanelAnchorPose)>,
    panels: Query<(Entity, &DiegeticPanel), With<ResolvedScreenPanelPosition>>,
    transforms: Query<&Transform>,
    mut resolved_positions: Query<&mut ResolvedScreenPanelPosition>,
    mut diagnostics: ResMut<AnchorResolveDiagnostics>,
) {
    let window_sizes = window::window_size_lookup(&windows);
    let mut desired_placements = placement::desired_placement_map(&panels);
    let mut depths = placement::panel_depths(&panels, &resolved_positions, &transforms);
    let anchor_poses = placement::panel_anchor_pose_map(&anchor_poses);
    let mut rects = rect::screen_panel_rects(
        &panels,
        &resolved_positions,
        &transforms,
        &primary,
        &window_sizes,
    );
    let candidates =
        candidate::classify_candidates(&attachments, &panels, &entities, &primary, &window_sizes)
            .into_iter()
            .filter(|candidate| match candidate {
                AttachmentResolveCandidate::Active { source, target, .. } => {
                    rects.contains_key(source) && rects.contains_key(target)
                },
                AttachmentResolveCandidate::Skipped { .. } => true,
            })
            .collect();
    let mut placer = ScreenAttachmentPlacer {
        rects:              &mut rects,
        desired_placements: &mut desired_placements,
        depths:             &mut depths,
        anchor_poses:       &anchor_poses,
    };
    panel::resolve_panel_attachments(
        candidates,
        placement::screen_attachment_resolve_reasons(),
        &mut diagnostics,
        |action| placer.handle(action),
    );
    placement::write_desired_placements(desired_placements, &mut resolved_positions);
}

#[cfg(test)]
#[allow(
    clippy::expect_used,
    reason = "tests should panic on unexpected values"
)]
mod tests {
    use std::f32::consts::FRAC_PI_2;
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
    use crate::PanelAnchorPose;
    use crate::PanelDimensionsChanged;
    use crate::PanelSystems;
    use crate::Pt;
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

    const ASSERT_CLOSE_EPSILON: f32 = 1e-4;

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
            (actual - expected).abs() < ASSERT_CLOSE_EPSILON,
            "expected {expected}, got {actual}",
        );
    }

    fn assert_vec2_close(actual: Vec2, expected: Vec2) {
        assert_close(actual.x, expected.x);
        assert_close(actual.y, expected.y);
    }

    fn assert_translation(app: &App, entity: Entity, expected: Vec2) {
        let transform = app
            .world()
            .get::<Transform>(entity)
            .expect("panel has transform");
        assert_close(transform.translation.x, expected.x);
        assert_close(transform.translation.y, expected.y);
    }

    fn assert_rotation_z(app: &App, entity: Entity, expected: f32) {
        let transform = app
            .world()
            .get::<Transform>(entity)
            .expect("panel has transform");
        assert_close(
            super::super::screen_in_plane_angle(transform.rotation),
            expected,
        );
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

    fn resolved_depth(app: &App, entity: Entity) -> Option<f32> {
        app.world()
            .get::<ResolvedScreenPanelPosition>(entity)
            .expect("panel has resolved position")
            .depth
    }

    fn authored_depth(app: &App, entity: Entity) -> Option<f32> {
        app.world()
            .get::<ResolvedScreenPanelPosition>(entity)
            .expect("panel has resolved position")
            .authored_depth
    }

    fn resolved_rotation(app: &App, entity: Entity) -> Option<f32> {
        app.world()
            .get::<ResolvedScreenPanelPosition>(entity)
            .expect("panel has resolved position")
            .rotation
    }

    fn authored_rotation(app: &App, entity: Entity) -> Option<f32> {
        app.world()
            .get::<ResolvedScreenPanelPosition>(entity)
            .expect("panel has resolved position")
            .authored_rotation
    }

    fn assert_depth(app: &App, entity: Entity, expected: f32) {
        let transform = app
            .world()
            .get::<Transform>(entity)
            .expect("panel has transform");
        assert_close(transform.translation.z, expected);
    }

    fn set_authored_depth(app: &mut App, entity: Entity, depth: f32) {
        app.world_mut()
            .get_mut::<Transform>(entity)
            .expect("panel has transform")
            .translation
            .z = depth;
    }

    fn set_authored_rotation(app: &mut App, entity: Entity, angle: f32) {
        app.world_mut()
            .get_mut::<Transform>(entity)
            .expect("panel has transform")
            .rotation = Quat::from_rotation_z(angle);
    }

    fn panel_anchor_screen_point(app: &App, entity: Entity, anchor: Anchor) -> Vec2 {
        let panel = app
            .world()
            .get::<DiegeticPanel>(entity)
            .expect("panel exists");
        let resolved_position = app
            .world()
            .get::<ResolvedScreenPanelPosition>(entity)
            .expect("panel has resolved position");
        let transform = app
            .world()
            .get::<Transform>(entity)
            .expect("panel has transform");
        let size = Vec2::new(panel.width(), panel.height());
        let panel_offset = anchor_offset(panel.anchor(), size);
        let anchor_offset = anchor_offset(anchor, size);
        resolved_position
            .anchor_position
            .expect("panel has resolved anchor position")
            + super::super::rotate_screen_offset(
                anchor_offset - panel_offset,
                super::super::screen_in_plane_angle(transform.rotation),
            )
    }

    fn anchor_offset(anchor: Anchor, size: Vec2) -> Vec2 {
        let (x, y) = anchor.offset(size.x, size.y);
        Vec2::new(x, y)
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
                    .with_offset(PanelAnchorOffset::new(Px(0.0), Px(1.0))),
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
    fn screen_attachment_applies_panel_anchor_pose_in_screen_plane() {
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
                    .with_offset(PanelAnchorOffset::new(Px(0.0), Px(1.0))),
                PanelAnchorPose {
                    rotation:    Quat::from_rotation_z(FRAC_PI_2),
                    translation: Vec3::new(10.0, 20.0, 30.0),
                },
            ))
            .id();

        app.update();

        assert_eq!(
            resolved_anchor_position(&app, source),
            Some(Vec2::new(110.0, 121.0))
        );
        assert_translation(&app, source, Vec2::new(-290.0, 179.0));
        assert_depth(&app, source, 30.0);
        assert_close(
            resolved_rotation(&app, source).expect("source has resolved rotation"),
            FRAC_PI_2,
        );
        assert_rotation_z(&app, source, FRAC_PI_2);
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
                    .with_offset(PanelAnchorOffset::new(Px(5.0), Px(2.0))),
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
    fn screen_pose_rotates_leaf_around_pinned_source_anchor() {
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
                fixed_screen_panel(Vec2::new(50.0, 10.0), Anchor::Center, Vec2::ZERO),
                AnchoredToPanel::new(target, Anchor::BottomRight, Anchor::BottomLeft),
                PanelAnchorPose {
                    rotation:    Quat::IDENTITY,
                    translation: Vec3::ZERO,
                },
            ))
            .id();

        app.update();

        let identity_pin = panel_anchor_screen_point(&app, source, Anchor::BottomRight);
        assert_vec2_close(identity_pin, Vec2::new(100.0, 140.0));

        app.world_mut().entity_mut(source).insert(PanelAnchorPose {
            rotation:    Quat::from_rotation_z(FRAC_PI_2),
            translation: Vec3::ZERO,
        });
        app.update();

        assert_eq!(
            resolved_anchor_position(&app, source),
            Some(Vec2::new(95.0, 165.0))
        );
        assert_vec2_close(
            panel_anchor_screen_point(&app, source, Anchor::BottomRight),
            identity_pin,
        );
        assert_rotation_z(&app, source, FRAC_PI_2);
    }

    #[test]
    fn out_of_plane_screen_pose_rotation_has_no_screen_effect() {
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
                fixed_screen_panel(Vec2::new(50.0, 10.0), Anchor::TopLeft, Vec2::ZERO),
                AnchoredToPanel::new(target, Anchor::TopLeft, Anchor::BottomLeft)
                    .with_offset(PanelAnchorOffset::new(Px(0.0), Px(1.0))),
                PanelAnchorPose {
                    rotation:    Quat::from_rotation_x(FRAC_PI_2),
                    translation: Vec3::ZERO,
                },
            ))
            .id();

        app.update();

        assert_eq!(
            resolved_anchor_position(&app, source),
            Some(Vec2::new(100.0, 141.0))
        );
        assert_close(
            resolved_rotation(&app, source).expect("source has resolved rotation"),
            0.0,
        );
        let transform = app
            .world()
            .get::<Transform>(source)
            .expect("panel has transform");
        assert_eq!(transform.rotation, Quat::IDENTITY);
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
                    .with_offset(PanelAnchorOffset::new(Px(0.0), Px(1.0))),
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
                    .with_offset(PanelAnchorOffset::new(Px(0.0), Px(1.0))),
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
    fn point_offsets_resolve_to_screen_pixels() {
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
                .with_offset(PanelAnchorOffset::new(Pt(12.0), Px(2.0))),
        );
        app.update();

        let resolved = app
            .world()
            .get::<ResolvedScreenPanelPosition>(source)
            .expect("source has resolved position");
        assert_eq!(resolved.anchor_position, Some(Vec2::new(116.0, 142.0)));
        assert_translation(&app, source, Vec2::new(-284.0, 158.0));
    }

    #[test]
    fn z_offset_writes_translation_depth_relative_to_target() {
        let mut app = app_with_window();
        let target = app
            .world_mut()
            .spawn(fixed_screen_panel(
                Vec2::new(200.0, 40.0),
                Anchor::TopLeft,
                Vec2::new(100.0, 100.0),
            ))
            .id();
        set_authored_depth(&mut app, target, 3.0);
        let source = app
            .world_mut()
            .spawn((
                fixed_screen_panel(Vec2::new(50.0, 10.0), Anchor::TopLeft, Vec2::new(0.0, 0.0)),
                AnchoredToPanel::new(target, Anchor::TopLeft, Anchor::BottomLeft)
                    .with_offset(PanelAnchorOffset::new(Px(0.0), Px(1.0)).with_z(Px(5.0))),
            ))
            .id();

        app.update();

        assert_translation(&app, source, Vec2::new(-300.0, 159.0));
        assert_depth(&app, source, 8.0);
        assert_eq!(resolved_depth(&app, source), Some(8.0));
    }

    #[test]
    fn cancelling_authored_z_inputs_still_own_depth() {
        let mut app = app_with_window();
        let target = app
            .world_mut()
            .spawn(fixed_screen_panel(
                Vec2::new(200.0, 40.0),
                Anchor::TopLeft,
                Vec2::new(100.0, 100.0),
            ))
            .id();
        set_authored_depth(&mut app, target, 3.0);
        let source = app
            .world_mut()
            .spawn((
                fixed_screen_panel(Vec2::new(50.0, 10.0), Anchor::TopLeft, Vec2::ZERO),
                AnchoredToPanel::new(target, Anchor::TopLeft, Anchor::BottomLeft)
                    .with_offset(PanelAnchorOffset::ZERO.with_z(Px(5.0))),
                PanelAnchorPose {
                    rotation:    Quat::IDENTITY,
                    translation: Vec3::new(0.0, 0.0, -5.0),
                },
            ))
            .id();
        set_authored_depth(&mut app, source, 7.0);

        app.update();

        assert_depth(&app, source, 3.0);
        assert_eq!(resolved_depth(&app, source), Some(3.0));
        assert_eq!(authored_depth(&app, source), Some(7.0));

        app.world_mut()
            .entity_mut(source)
            .insert(AnchoredToPanel::new(
                target,
                Anchor::TopLeft,
                Anchor::BottomLeft,
            ))
            .remove::<PanelAnchorPose>();
        app.update();

        assert_eq!(resolved_depth(&app, source), None);
        assert_eq!(authored_depth(&app, source), None);
        assert_depth(&app, source, 7.0);
    }

    #[test]
    fn z_offset_chain_accumulates_depth_in_one_update() {
        let mut app = app_with_window();
        let root = app
            .world_mut()
            .spawn(fixed_screen_panel(
                Vec2::new(200.0, 40.0),
                Anchor::TopLeft,
                Vec2::new(100.0, 100.0),
            ))
            .id();
        let middle = app
            .world_mut()
            .spawn((
                fixed_screen_panel(Vec2::new(50.0, 10.0), Anchor::TopLeft, Vec2::ZERO),
                AnchoredToPanel::new(root, Anchor::TopLeft, Anchor::BottomLeft)
                    .with_offset(PanelAnchorOffset::ZERO.with_z(Px(2.0))),
            ))
            .id();
        let leaf = app
            .world_mut()
            .spawn((
                fixed_screen_panel(Vec2::new(25.0, 10.0), Anchor::TopLeft, Vec2::ZERO),
                AnchoredToPanel::new(middle, Anchor::TopLeft, Anchor::BottomLeft)
                    .with_offset(PanelAnchorOffset::ZERO.with_z(Px(3.0))),
            ))
            .id();

        app.update();

        assert_depth(&app, middle, 2.0);
        assert_depth(&app, leaf, 5.0);
    }

    #[test]
    fn removing_attachment_restores_authored_depth() {
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
                fixed_screen_panel(Vec2::new(50.0, 10.0), Anchor::TopLeft, Vec2::ZERO),
                AnchoredToPanel::new(target, Anchor::TopLeft, Anchor::BottomLeft)
                    .with_offset(PanelAnchorOffset::ZERO.with_z(Px(5.0))),
            ))
            .id();
        set_authored_depth(&mut app, source, 7.0);

        app.update();
        assert_depth(&app, source, 5.0);
        assert_eq!(resolved_depth(&app, source), Some(5.0));

        app.world_mut()
            .entity_mut(source)
            .remove::<AnchoredToPanel>();
        app.update();

        assert_eq!(resolved_depth(&app, source), None);
        assert_depth(&app, source, 7.0);
    }

    #[test]
    fn removing_panel_anchor_pose_restores_authored_rotation() {
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
                fixed_screen_panel(Vec2::new(50.0, 10.0), Anchor::TopLeft, Vec2::ZERO),
                AnchoredToPanel::new(target, Anchor::TopLeft, Anchor::BottomLeft),
                PanelAnchorPose {
                    rotation:    Quat::from_rotation_z(FRAC_PI_2),
                    translation: Vec3::ZERO,
                },
            ))
            .id();
        set_authored_rotation(&mut app, source, 0.0);

        app.update();
        assert_close(
            resolved_rotation(&app, source).expect("source has resolved rotation"),
            FRAC_PI_2,
        );
        assert_eq!(authored_rotation(&app, source), Some(0.0));
        assert_rotation_z(&app, source, FRAC_PI_2);

        app.world_mut()
            .entity_mut(source)
            .remove::<PanelAnchorPose>();
        app.update();

        assert_eq!(resolved_rotation(&app, source), None);
        assert_eq!(authored_rotation(&app, source), None);
        assert_rotation_z(&app, source, 0.0);
    }

    #[test]
    fn zero_z_offset_keeps_authored_depth_untouched() {
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
                fixed_screen_panel(Vec2::new(50.0, 10.0), Anchor::TopLeft, Vec2::ZERO),
                AnchoredToPanel::new(target, Anchor::TopLeft, Anchor::BottomLeft)
                    .with_offset(PanelAnchorOffset::new(Px(0.0), Px(1.0))),
            ))
            .id();
        set_authored_depth(&mut app, source, 7.0);

        app.update();

        assert_translation(&app, source, Vec2::new(-300.0, 159.0));
        assert_eq!(resolved_depth(&app, source), None);
        assert_depth(&app, source, 7.0);
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
    fn target_anchor_offset_uses_rotated_target_frame() {
        let mut app = app_with_window();
        let root = app
            .world_mut()
            .spawn(fixed_screen_panel(
                Vec2::new(100.0, 20.0),
                Anchor::TopLeft,
                Vec2::new(100.0, 100.0),
            ))
            .id();
        let target = app
            .world_mut()
            .spawn((
                fixed_screen_panel(Vec2::new(50.0, 10.0), Anchor::TopLeft, Vec2::ZERO),
                AnchoredToPanel::new(root, Anchor::TopLeft, Anchor::TopLeft),
                PanelAnchorPose {
                    rotation:    Quat::from_rotation_z(FRAC_PI_2),
                    translation: Vec3::ZERO,
                },
            ))
            .id();
        let source = app
            .world_mut()
            .spawn((
                fixed_screen_panel(Vec2::new(25.0, 10.0), Anchor::TopLeft, Vec2::ZERO),
                AnchoredToPanel::new(target, Anchor::TopLeft, Anchor::TopLeft)
                    .with_offset(PanelAnchorOffset::new(Px(5.0), Px(2.0))),
            ))
            .id();

        app.update();

        let target_pin = panel_anchor_screen_point(&app, target, Anchor::TopLeft);
        let expected_pin =
            target_pin + super::super::rotate_screen_offset(Vec2::new(5.0, 2.0), FRAC_PI_2);
        let source_pin = panel_anchor_screen_point(&app, source, Anchor::TopLeft);
        assert_vec2_close(source_pin, expected_pin);
        assert_vec2_close(
            resolved_anchor_position(&app, source).expect("source has resolved position"),
            expected_pin,
        );
        assert_translation(&app, source, Vec2::new(-298.0, 205.0));
    }

    #[test]
    fn rotated_screen_target_repositions_dependent_in_same_chain_update() {
        let mut app = app_with_window();
        let root = app
            .world_mut()
            .spawn(fixed_screen_panel(
                Vec2::new(100.0, 20.0),
                Anchor::TopLeft,
                Vec2::new(100.0, 100.0),
            ))
            .id();
        let middle = app
            .world_mut()
            .spawn((
                fixed_screen_panel(Vec2::new(50.0, 10.0), Anchor::TopLeft, Vec2::ZERO),
                AnchoredToPanel::new(root, Anchor::TopLeft, Anchor::BottomLeft),
                PanelAnchorPose {
                    rotation:    Quat::from_rotation_z(FRAC_PI_2),
                    translation: Vec3::ZERO,
                },
            ))
            .id();
        let leaf = app
            .world_mut()
            .spawn((
                fixed_screen_panel(Vec2::new(25.0, 10.0), Anchor::TopLeft, Vec2::ZERO),
                AnchoredToPanel::new(middle, Anchor::TopLeft, Anchor::BottomLeft),
                PanelAnchorPose {
                    rotation:    Quat::from_rotation_z(-FRAC_PI_2),
                    translation: Vec3::ZERO,
                },
            ))
            .id();

        app.update();

        assert_eq!(
            resolved_anchor_position(&app, middle),
            Some(Vec2::new(100.0, 120.0))
        );
        assert_eq!(
            resolved_anchor_position(&app, leaf),
            Some(Vec2::new(110.0, 120.0))
        );
        assert_translation(&app, middle, Vec2::new(-300.0, 180.0));
        assert_translation(&app, leaf, Vec2::new(-290.0, 180.0));
        assert_rotation_z(&app, middle, FRAC_PI_2);
        assert_rotation_z(&app, leaf, -FRAC_PI_2);
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
