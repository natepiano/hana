//! Diegetic-panel adapter for `hana_valence` arrangements.

use bevy::prelude::*;
use hana_valence::Accordion;
use hana_valence::AnchorPose;
use hana_valence::AnchoredTo;
use hana_valence::ArrangementMembers;
use hana_valence::Coil;
use hana_valence::Hinge;
use hana_valence::Member;
use hana_valence::MemberIndex;
use hana_valence::PendingMemberPlacement;
use hana_valence::QuadTiling;
use hana_valence::ResolvedAnchorGeometry;
use hana_valence::Strip;

use super::DiegeticPanel;
use super::lifecycle;
use super::valence_provider;

/// Insert-only bundle that makes a panel a member of a valence arrangement.
#[derive(Bundle, Clone, Copy, Debug, PartialEq, Eq)]
pub struct ArrangedPanel {
    member: Member,
}

impl ArrangedPanel {
    /// Creates an arrangement member that belongs to `arrangement`.
    #[must_use]
    pub const fn new(arrangement: Entity) -> Self {
        Self {
            member: Member { arrangement },
        }
    }

    /// Arrangement entity.
    #[must_use]
    pub const fn arrangement(&self) -> Entity { self.member.arrangement }
}

/// Marks attachment, pose, and hinge state installed by panel arrangement placement.
#[derive(Component)]
pub(super) struct PanelArrangementRuntime;

pub(super) fn apply_panel_member_placements(
    mut commands: Commands,
    pending: Query<
        (
            Entity,
            &Member,
            &MemberIndex,
            &DiegeticPanel,
            Option<&ResolvedAnchorGeometry>,
        ),
        (
            With<PendingMemberPlacement>,
            With<Transform>,
            With<GlobalTransform>,
        ),
    >,
    members: Query<&ArrangementMembers>,
    rules: Query<&QuadTiling>,
    accordions: Query<&Accordion>,
    coils: Query<&Coil>,
    strips: Query<&Strip>,
    panel_targets: Query<
        (&DiegeticPanel, Option<&ResolvedAnchorGeometry>),
        (With<Transform>, With<GlobalTransform>),
    >,
    ready: Query<
        (),
        (
            With<ResolvedAnchorGeometry>,
            With<Transform>,
            With<GlobalTransform>,
        ),
    >,
) {
    for (entity, member, index, panel, geometry) in &pending {
        let Ok(arrangement_members) = members.get(member.arrangement) else {
            continue;
        };
        let Ok(rule) = rules.get(member.arrangement) else {
            continue;
        };
        let Some(placement) = hana_valence::member_placement(
            entity,
            *member,
            *index,
            arrangement_members,
            rule,
            accordions.get(member.arrangement).ok(),
            coils.get(member.arrangement).ok(),
            strips.get(member.arrangement).ok(),
        ) else {
            continue;
        };
        if !placement_target_ready(
            placement.placement.anchored_to.target(),
            &panel_targets,
            &ready,
            &mut commands,
        ) {
            continue;
        }
        ensure_panel_anchor_geometry(&mut commands, entity, panel, geometry);
        commands.entity(entity).insert((
            placement.placement.anchored_to,
            AnchorPose::default(),
            Hinge {
                edge:  placement.placement.hinge_edge,
                angle: placement.angle,
            },
            PanelArrangementRuntime,
        ));
        commands.entity(entity).remove::<PendingMemberPlacement>();
    }
}

/// Removes runtime placement state when a panel stops participating in an arrangement.
pub(super) fn cleanup_panel_member_placement(
    removed: On<Remove, Member>,
    placements: Query<(), With<PanelArrangementRuntime>>,
    mut commands: Commands,
) {
    let entity = removed.entity;
    if !placements.contains(entity) {
        return;
    }
    commands
        .entity(entity)
        .remove::<(AnchoredTo, AnchorPose, Hinge, PanelArrangementRuntime)>();
}

fn placement_target_ready(
    target: Entity,
    panel_targets: &Query<
        (&DiegeticPanel, Option<&ResolvedAnchorGeometry>),
        (With<Transform>, With<GlobalTransform>),
    >,
    ready: &Query<
        (),
        (
            With<ResolvedAnchorGeometry>,
            With<Transform>,
            With<GlobalTransform>,
        ),
    >,
    commands: &mut Commands,
) -> bool {
    if ready.contains(target) {
        return true;
    }
    let Ok((panel, geometry)) = panel_targets.get(target) else {
        return false;
    };
    ensure_panel_anchor_geometry(commands, target, panel, geometry);
    true
}

fn ensure_panel_anchor_geometry(
    commands: &mut Commands,
    entity: Entity,
    panel: &DiegeticPanel,
    geometry: Option<&ResolvedAnchorGeometry>,
) {
    if geometry.is_none() {
        lifecycle::write_owned_component(
            commands,
            entity,
            entity,
            valence_provider::panel_anchor_geometry(panel),
        );
    }
}

#[cfg(test)]
#[allow(
    clippy::expect_used,
    reason = "tests should panic on unexpected values"
)]
mod tests {
    use std::sync::Arc;

    use bevy::prelude::*;
    use bevy::transform::TransformPlugin;
    use bevy::window::PrimaryWindow;
    use bevy::window::Window;
    use bevy::window::WindowResolution;
    use hana_valence::Accordion;
    use hana_valence::AnchorId;
    use hana_valence::AnchoredTo as ValenceAnchoredTo;
    use hana_valence::QuadTiling;
    use hana_valence::ResolveDiagnostics;
    use hana_valence::ResolvedAnchorGeometry;

    use super::ArrangedPanel;
    use crate::HeadlessLayoutPlugin;
    use crate::Px;
    use crate::layout::Anchor;
    use crate::layout::TextDimensions;
    use crate::layout::TextMeasure;
    use crate::panel::DiegeticPanel;
    use crate::panel::PanelAttachmentAuthored;
    use crate::panel::PanelSpace;
    use crate::screen_space::ScreenSpacePlugin;
    use crate::text::DiegeticTextMeasurer;

    const ASSERT_EPSILON: f32 = 1e-4;
    const FULL_FOLD: f32 = 1.0;
    const HALF_WINDOW_FACTOR: f32 = 0.5;
    const SCREEN_PANEL_HEIGHT: f32 = 40.0;
    const SCREEN_PANEL_POSITION: f32 = 10.0;
    const SCREEN_PANEL_WIDTH: f32 = 100.0;
    const WINDOW_HEIGHT: f32 = 600.0;
    const WINDOW_HEIGHT_PIXELS: u32 = 600;
    const WINDOW_WIDTH: f32 = 800.0;
    const WINDOW_WIDTH_PIXELS: u32 = 800;

    #[test]
    fn screen_arranged_panel_never_enters_valence_resolver() {
        let mut app = app_with_panel_arrangements();
        let arrangement = app
            .world_mut()
            .spawn((
                screen_panel(),
                Transform::default(),
                Accordion {
                    fold: FULL_FOLD,
                    lean: core::f32::consts::FRAC_PI_2,
                },
                QuadTiling,
            ))
            .id();
        let first_member = app
            .world_mut()
            .spawn((
                world_member_panel(),
                Transform::default(),
                ArrangedPanel::new(arrangement),
            ))
            .id();
        let second_member = app
            .world_mut()
            .spawn((
                world_member_panel(),
                Transform::default(),
                ArrangedPanel::new(arrangement),
            ))
            .id();

        run_arrangement_frames(&mut app);

        assert_eq!(
            app.world().get::<PanelSpace>(arrangement),
            Some(&PanelSpace::Screen)
        );
        assert_eq!(
            app.world()
                .get::<ValenceAnchoredTo>(first_member)
                .map(ValenceAnchoredTo::target),
            Some(arrangement),
        );
        assert_eq!(
            app.world()
                .get::<ValenceAnchoredTo>(second_member)
                .map(ValenceAnchoredTo::target),
            Some(first_member),
        );
        assert!(app.world().get::<ValenceAnchoredTo>(arrangement).is_none());
        assert!(
            app.world()
                .get::<PanelAttachmentAuthored>(first_member)
                .is_none()
        );
        assert!(
            app.world()
                .get::<PanelAttachmentAuthored>(second_member)
                .is_none()
        );
        assert_vec3_close(
            transform(&app, arrangement).translation,
            Vec3::new(
                WINDOW_WIDTH.mul_add(-HALF_WINDOW_FACTOR, SCREEN_PANEL_POSITION),
                WINDOW_HEIGHT.mul_add(HALF_WINDOW_FACTOR, -SCREEN_PANEL_POSITION),
                0.0,
            ),
        );
        assert_connected(&app, first_member, arrangement);
        assert_connected(&app, second_member, first_member);
        assert_quat_close(
            transform(&app, first_member).rotation,
            Quat::from_rotation_x(core::f32::consts::FRAC_PI_2),
        );
        assert_quat_close(transform(&app, second_member).rotation, Quat::IDENTITY);
        let diagnostics = app.world().resource::<ResolveDiagnostics>();
        assert!(diagnostics.current().next().is_none());
    }

    fn run_arrangement_frames(app: &mut App) {
        app.update();
        app.update();
        app.update();
    }

    fn app_with_panel_arrangements() -> App {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        app.add_plugins(TransformPlugin);
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
                resolution: WindowResolution::new(WINDOW_WIDTH_PIXELS, WINDOW_HEIGHT_PIXELS),
                ..Default::default()
            },
            PrimaryWindow,
        ));
        app
    }

    fn screen_panel() -> DiegeticPanel {
        DiegeticPanel::screen()
            .size(Px(SCREEN_PANEL_WIDTH), Px(SCREEN_PANEL_HEIGHT))
            .screen_position(SCREEN_PANEL_POSITION, SCREEN_PANEL_POSITION)
            .layout(|_| {})
            .build()
            .expect("screen panel builds")
    }

    fn world_member_panel() -> DiegeticPanel {
        DiegeticPanel::world()
            .size(Px(SCREEN_PANEL_WIDTH), Px(SCREEN_PANEL_HEIGHT))
            .world_height(SCREEN_PANEL_HEIGHT)
            .layout(|_| {})
            .build()
            .expect("world panel builds")
    }

    fn assert_connected(app: &App, source: Entity, target: Entity) {
        assert_vec3_close(
            anchor_world_position(app, source, Anchor::TopCenter),
            anchor_world_position(app, target, Anchor::BottomCenter),
        );
    }

    fn anchor_world_position(app: &App, entity: Entity, anchor: Anchor) -> Vec3 {
        let geometry = app
            .world()
            .get::<ResolvedAnchorGeometry>(entity)
            .expect("panel has anchor geometry");
        let point = geometry
            .points
            .get(&AnchorId::from(anchor))
            .expect("panel has anchor point");
        app.world()
            .get::<GlobalTransform>(entity)
            .expect("panel has global transform")
            .transform_point(point.position)
    }

    fn transform(app: &App, entity: Entity) -> &Transform {
        app.world()
            .get::<Transform>(entity)
            .expect("panel has transform")
    }

    fn assert_vec3_close(actual: Vec3, expected: Vec3) {
        assert!(
            actual.abs_diff_eq(expected, ASSERT_EPSILON),
            "expected {expected:?}, got {actual:?}",
        );
    }

    fn assert_quat_close(actual: Quat, expected: Quat) {
        assert!(
            actual.dot(expected).abs() > 1.0 - ASSERT_EPSILON,
            "expected {expected:?}, got {actual:?}",
        );
    }
}
