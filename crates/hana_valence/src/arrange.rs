//! Ordered tiling arrangements for anchor relationships.

use bevy_ecs::entity::Entity;
use bevy_ecs::prelude::Add;
use bevy_ecs::prelude::Commands;
use bevy_ecs::prelude::Component;
use bevy_ecs::prelude::On;
use bevy_ecs::prelude::ReflectComponent;
use bevy_ecs::prelude::Remove;
use bevy_ecs::query::Or;
use bevy_ecs::query::With;
use bevy_ecs::query::Without;
use bevy_ecs::system::Query;
use bevy_platform::collections::HashMap;
use bevy_reflect::Reflect;
use bevy_reflect::std_traits::ReflectDefault;
use bevy_transform::prelude::GlobalTransform;
use bevy_transform::prelude::Transform;

use crate::AnchorId;
use crate::AnchorPose;
use crate::AnchoredTo;
use crate::Edge;
use crate::FoldAngles;
use crate::Hinge;
use crate::ResolvedAnchorGeometry;

const ACCORDION_SIGN_PERIOD: usize = 2;
const FIRST_MEMBER_INDEX: usize = 1;
const NEGATIVE_FOLD_SIGN: f32 = -1.0;
const POSITIVE_FOLD_SIGN: f32 = 1.0;
const QUAD_BOTTOM_EDGE_MIDPOINT: u32 = 2;
const QUAD_BOTTOM_LEFT_VERTEX: u32 = 3;
const QUAD_BOTTOM_RIGHT_VERTEX: u32 = 2;
const QUAD_LEFT_EDGE_MIDPOINT: u32 = 3;
const QUAD_RIGHT_EDGE_MIDPOINT: u32 = 1;
const QUAD_TOP_EDGE_MIDPOINT: u32 = 0;
const QUAD_TOP_LEFT_VERTEX: u32 = 0;
const QUAD_TOP_RIGHT_VERTEX: u32 = 1;

const QUAD_BOTTOM_EDGE: Edge = Edge {
    start: AnchorId::Vertex(QUAD_BOTTOM_RIGHT_VERTEX),
    end:   AnchorId::Vertex(QUAD_BOTTOM_LEFT_VERTEX),
};
const QUAD_TOP_EDGE: Edge = Edge {
    start: AnchorId::Vertex(QUAD_TOP_LEFT_VERTEX),
    end:   AnchorId::Vertex(QUAD_TOP_RIGHT_VERTEX),
};
const QUAD_LEFT_EDGE: Edge = Edge {
    start: AnchorId::Vertex(QUAD_BOTTOM_LEFT_VERTEX),
    end:   AnchorId::Vertex(QUAD_TOP_LEFT_VERTEX),
};
const QUAD_RIGHT_EDGE: Edge = Edge {
    start: AnchorId::Vertex(QUAD_TOP_RIGHT_VERTEX),
    end:   AnchorId::Vertex(QUAD_BOTTOM_RIGHT_VERTEX),
};

/// Drivable fold arrangement over an ordered member set.
///
/// `fold` is clamped to `0..=1` when computing member hinge angles. `lean` is
/// the fold angle, in radians, at `fold == 1`. Adjacent member hinges alternate
/// signs so the members fold as an accordion.
#[derive(Component, Clone, Copy, Debug, PartialEq, Reflect)]
#[reflect(Component, PartialEq, Debug, Clone)]
pub struct Accordion {
    /// Fold amount, interpreted as `0..=1`.
    pub fold: f32,
    /// Fold angle in radians at full fold.
    pub lean: f32,
}

impl Default for Accordion {
    fn default() -> Self {
        Self {
            fold: 0.0,
            lean: core::f32::consts::PI,
        }
    }
}

impl Accordion {
    /// Returns the fold term added to the tiling rule's rest angle for `index`.
    #[must_use]
    pub fn fold_contribution(self, index: usize) -> f32 {
        let sign = if index.is_multiple_of(ACCORDION_SIGN_PERIOD) {
            NEGATIVE_FOLD_SIGN
        } else {
            POSITIVE_FOLD_SIGN
        };
        fold_angle(self.fold, self.lean) * sign
    }
}

/// Drivable coil arrangement over an ordered member set.
///
/// `fold` is clamped to `0..=1` when computing member hinge angles. `lean` is
/// the fold angle, in radians, at `fold == 1`. Every member hinge uses the same
/// sign, so world rotations accumulate down the member set.
#[derive(Component, Clone, Copy, Debug, PartialEq, Reflect)]
#[reflect(Component, PartialEq, Debug, Clone)]
pub struct Coil {
    /// Fold amount, interpreted as `0..=1`.
    pub fold: f32,
    /// Fold angle in radians at full fold.
    pub lean: f32,
}

impl Default for Coil {
    fn default() -> Self {
        Self {
            fold: 0.0,
            lean: core::f32::consts::PI,
        }
    }
}

impl Coil {
    /// Returns the fold term added to every member's tiling-rule rest angle.
    #[must_use]
    pub fn fold_contribution(self) -> f32 { fold_angle(self.fold, self.lean) }
}

/// Static straight arrangement over an ordered member set.
#[derive(Component, Clone, Copy, Debug, Default, Eq, PartialEq, Reflect)]
#[reflect(Component, Default, PartialEq, Debug, Clone)]
pub struct Strip;

/// Marker assigning an entity to an arrangement entity.
#[derive(Component, Clone, Copy, Debug, Eq, PartialEq, Reflect)]
#[reflect(Component, PartialEq, Debug, Clone)]
pub struct Member {
    /// Arrangement entity that owns this member's order and placement rule.
    #[entities]
    pub arrangement: Entity,
}

/// Index assigned to a [`Member`] within its arrangement.
#[derive(Component, Clone, Copy, Debug, Eq, PartialEq, Reflect)]
#[reflect(Component, PartialEq, Debug, Clone)]
pub struct MemberIndex {
    /// Stable order index used by tiling and fold rules.
    pub index: usize,
}

/// Ordered member list tracked on an arrangement entity.
#[derive(Component, Clone, Debug, Default, Eq, PartialEq, Reflect)]
#[reflect(Component, Default, PartialEq, Debug, Clone)]
pub struct ArrangementMembers {
    entities: Vec<Entity>,
}

impl ArrangementMembers {
    /// Iterates over members in insertion order.
    pub fn iter(&self) -> impl Iterator<Item = Entity> + '_ { self.entities.iter().copied() }

    /// Number of members currently tracked.
    #[must_use]
    pub const fn len(&self) -> usize { self.entities.len() }

    /// Whether this arrangement has no tracked members.
    #[must_use]
    pub const fn is_empty(&self) -> bool { self.entities.is_empty() }

    /// Returns the entity preceding `member`, or `arrangement` for the first member.
    #[must_use]
    pub fn predecessor(&self, arrangement: Entity, member: Entity) -> Option<Entity> {
        let mut predecessor = arrangement;
        for entity in &self.entities {
            if *entity == member {
                return Some(predecessor);
            }
            predecessor = *entity;
        }
        None
    }

    fn clean(&mut self, indexes: &Query<&MemberIndex>) {
        self.entities.retain(|entity| indexes.contains(*entity));
    }

    fn push(&mut self, entity: Entity) {
        if !self.entities.contains(&entity) {
            self.entities.push(entity);
        }
    }

    fn without(mut self, entity: Entity) -> Self {
        self.entities.retain(|member| *member != entity);
        self
    }

    fn next_index(&self, indexes: &Query<&MemberIndex>) -> usize {
        self.entities
            .iter()
            .filter_map(|entity| indexes.get(*entity).ok())
            .map(|index| index.index)
            .max()
            .map_or(FIRST_MEMBER_INDEX, |index| index + 1)
    }
}

/// Marker for a member whose consumer-specific placement has not been applied.
#[derive(Component, Clone, Copy, Debug, Default, Eq, PartialEq, Reflect)]
#[reflect(Component, Default, PartialEq, Debug, Clone)]
pub struct PendingMemberPlacement;

/// Geometry-specific tiling contract used by arrangements.
///
/// This trait is a downstream extension point: reusable arrangements call the
/// rule instead of hardcoding provider anchor ids.
pub trait TilingRule {
    /// Returns the source edge on member `index` and target edge on its predecessor.
    fn next_edge(&self, index: usize) -> (Edge, Edge);

    /// Returns the anchor point that represents `edge` for this geometry.
    fn edge_anchor(&self, edge: Edge) -> Option<AnchorId>;

    /// Computes the relation and rest hinge data for member `index`.
    fn placement(&self, target: Entity, index: usize) -> Option<ArrangementPlacement> {
        let (source_edge, target_edge) = self.next_edge(index);
        Some(ArrangementPlacement {
            anchored_to: AnchoredTo::new(
                target,
                self.edge_anchor(source_edge)?,
                self.edge_anchor(target_edge)?,
            ),
            hinge_edge:  source_edge,
            rest_angle:  self.rest_delta(index),
        })
    }

    /// Rest angle in radians around the shared hinge edge for member `index`.
    fn rest_delta(&self, index: usize) -> f32;
}

/// Quad tiling rule used by in-crate arrangements and tests.
#[derive(Component, Clone, Copy, Debug, Default, Eq, PartialEq, Reflect)]
#[reflect(Component, Default, PartialEq, Debug, Clone)]
pub struct QuadTiling;

impl TilingRule for QuadTiling {
    fn next_edge(&self, _: usize) -> (Edge, Edge) { (QUAD_TOP_EDGE, QUAD_BOTTOM_EDGE) }

    fn edge_anchor(&self, edge: Edge) -> Option<AnchorId> { quad_edge_anchor(edge) }

    fn rest_delta(&self, _: usize) -> f32 { 0.0 }
}

/// Placement data emitted by an arrangement for one member.
#[derive(Clone, Copy, Debug, PartialEq, Reflect)]
#[reflect(PartialEq, Debug, Clone)]
pub struct ArrangementPlacement {
    /// Raw valence relation for consumers that place entities directly.
    pub anchored_to: AnchoredTo,
    /// Member-local edge used by [`Hinge`] for fold rotation.
    pub hinge_edge:  Edge,
    /// Rest angle in radians before arrangement fold contribution is added.
    pub rest_angle:  f32,
}

impl ArrangementPlacement {
    const fn with_angle(self, angle: f32) -> MemberPlacement {
        MemberPlacement {
            placement: self,
            angle,
        }
    }
}

/// Placement data plus the live hinge angle for one member.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MemberPlacement {
    /// Placement data emitted by the arrangement rule.
    pub placement: ArrangementPlacement,
    /// Hinge angle in radians after rest angle and fold contribution.
    pub angle:     f32,
}

/// Assigns a stable index to a newly added [`Member`].
pub fn on_member_added(
    added: On<Add, Member>,
    members: Query<&Member>,
    indexes: Query<&MemberIndex>,
    arrangements: Query<
        Option<&ArrangementMembers>,
        Or<(With<Accordion>, With<Coil>, With<Strip>)>,
    >,
    mut commands: Commands,
) {
    let entity = added.entity;
    let Ok(member) = members.get(entity) else {
        return;
    };
    assign_one_member(entity, *member, &indexes, &arrangements, &mut commands);
}

/// Removes a deleted [`Member`] from its arrangement's ordered member list.
pub fn on_member_removed(
    removed: On<Remove, Member>,
    members: Query<&Member>,
    arrangements: Query<&ArrangementMembers>,
    mut commands: Commands,
) {
    let entity = removed.entity;
    let Ok(member) = members.get(entity) else {
        return;
    };
    let Ok(members) = arrangements.get(member.arrangement) else {
        commands
            .entity(entity)
            .remove::<(MemberIndex, PendingMemberPlacement)>();
        return;
    };
    commands
        .entity(member.arrangement)
        .insert(members.clone().without(entity));
    commands
        .entity(entity)
        .remove::<(MemberIndex, PendingMemberPlacement)>();
}

/// Assigns indices to [`Member`] components that do not yet have one.
pub fn assign_member_indices(
    members: Query<(Entity, &Member), Without<MemberIndex>>,
    indexes: Query<&MemberIndex>,
    arrangements: Query<
        Option<&ArrangementMembers>,
        Or<(With<Accordion>, With<Coil>, With<Strip>)>,
    >,
    mut commands: Commands,
) {
    let mut updates = HashMap::<Entity, ArrangementMembers>::default();
    let mut next_indices = HashMap::<Entity, usize>::default();
    for (entity, member) in &members {
        assign_one_member_with_updates(
            entity,
            *member,
            &indexes,
            &arrangements,
            &mut updates,
            &mut next_indices,
            &mut commands,
        );
    }
    for (arrangement, members) in updates {
        commands.entity(arrangement).insert(members);
    }
}

/// Applies pending placements for plain valence entities.
pub fn apply_member_placements<R: Component + TilingRule>(
    mut commands: Commands,
    pending: Query<
        (Entity, &Member, &MemberIndex),
        (
            With<PendingMemberPlacement>,
            With<ResolvedAnchorGeometry>,
            With<Transform>,
            With<GlobalTransform>,
        ),
    >,
    members: Query<&ArrangementMembers>,
    rules: Query<&R>,
    accordions: Query<&Accordion>,
    coils: Query<&Coil>,
    strips: Query<&Strip>,
    ready: Query<
        (),
        (
            With<ResolvedAnchorGeometry>,
            With<Transform>,
            With<GlobalTransform>,
        ),
    >,
) {
    for (entity, member, index) in &pending {
        let Ok(arrangement_members) = members.get(member.arrangement) else {
            continue;
        };
        let Ok(rule) = rules.get(member.arrangement) else {
            continue;
        };
        let Some(placement) = member_placement(
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
        if !ready.contains(placement.placement.anchored_to.target()) {
            continue;
        }
        commands.entity(entity).insert((
            placement.placement.anchored_to,
            AnchorPose::default(),
            Hinge {
                edge:  placement.placement.hinge_edge,
                angle: placement.angle,
            },
        ));
        commands.entity(entity).remove::<PendingMemberPlacement>();
    }
}

/// Writes arrangement angles to member hinges that do not carry [`FoldAngles`].
pub fn drive_arrangement_hinges<R: Component + TilingRule>(
    mut hinges: Query<(&Member, &MemberIndex, &mut Hinge), Without<FoldAngles>>,
    rules: Query<&R>,
    accordions: Query<&Accordion>,
    coils: Query<&Coil>,
    strips: Query<&Strip>,
) {
    for (member, index, mut hinge) in &mut hinges {
        let Ok(rule) = rules.get(member.arrangement) else {
            continue;
        };
        let Some(angle) = arrangement_angle(
            index.index,
            rule,
            accordions.get(member.arrangement).ok(),
            coils.get(member.arrangement).ok(),
            strips.get(member.arrangement).ok(),
        ) else {
            continue;
        };
        hinge.angle = angle;
    }
}

/// Computes placement data for one indexed member.
#[must_use]
pub fn member_placement(
    entity: Entity,
    member: Member,
    index: MemberIndex,
    members: &ArrangementMembers,
    rule: &dyn TilingRule,
    accordion: Option<&Accordion>,
    coil: Option<&Coil>,
    strip: Option<&Strip>,
) -> Option<MemberPlacement> {
    let target = members.predecessor(member.arrangement, entity)?;
    let placement = rule.placement(target, index.index)?;
    let angle = arrangement_angle(index.index, rule, accordion, coil, strip)?;
    Some(placement.with_angle(angle))
}

fn assign_one_member(
    entity: Entity,
    member: Member,
    indexes: &Query<&MemberIndex>,
    arrangements: &Query<
        Option<&ArrangementMembers>,
        Or<(With<Accordion>, With<Coil>, With<Strip>)>,
    >,
    commands: &mut Commands,
) {
    let mut updates = HashMap::<Entity, ArrangementMembers>::default();
    let mut next_indices = HashMap::<Entity, usize>::default();
    assign_one_member_with_updates(
        entity,
        member,
        indexes,
        arrangements,
        &mut updates,
        &mut next_indices,
        commands,
    );
    for (arrangement, members) in updates {
        commands.entity(arrangement).insert(members);
    }
}

fn assign_one_member_with_updates(
    entity: Entity,
    member: Member,
    indexes: &Query<&MemberIndex>,
    arrangements: &Query<
        Option<&ArrangementMembers>,
        Or<(With<Accordion>, With<Coil>, With<Strip>)>,
    >,
    updates: &mut HashMap<Entity, ArrangementMembers>,
    next_indices: &mut HashMap<Entity, usize>,
    commands: &mut Commands,
) {
    let Ok(existing) = arrangements.get(member.arrangement) else {
        return;
    };
    let members = updates.entry(member.arrangement).or_insert_with(|| {
        let mut members = existing.cloned().unwrap_or_default();
        members.clean(indexes);
        members
    });
    let index = next_indices
        .entry(member.arrangement)
        .or_insert_with(|| members.next_index(indexes));
    let member_index = *index;
    *index = (*index).saturating_add(1);
    members.push(entity);
    commands.entity(entity).insert((
        MemberIndex {
            index: member_index,
        },
        PendingMemberPlacement,
    ));
}

fn arrangement_angle(
    index: usize,
    rule: &dyn TilingRule,
    accordion: Option<&Accordion>,
    coil: Option<&Coil>,
    strip: Option<&Strip>,
) -> Option<f32> {
    let contribution = match (accordion, coil, strip) {
        (Some(accordion), None, None) => accordion.fold_contribution(index),
        (None, Some(coil), None) => coil.fold_contribution(),
        (None, None, Some(_)) => 0.0,
        _ => return None,
    };
    Some(rule.rest_delta(index) + contribution)
}

fn fold_angle(fold: f32, lean: f32) -> f32 { fold.clamp(0.0, 1.0) * lean }

fn quad_edge_anchor(edge: Edge) -> Option<AnchorId> {
    if edge == QUAD_TOP_EDGE {
        Some(AnchorId::EdgeMid(QUAD_TOP_EDGE_MIDPOINT))
    } else if edge == QUAD_RIGHT_EDGE {
        Some(AnchorId::EdgeMid(QUAD_RIGHT_EDGE_MIDPOINT))
    } else if edge == QUAD_BOTTOM_EDGE {
        Some(AnchorId::EdgeMid(QUAD_BOTTOM_EDGE_MIDPOINT))
    } else if edge == QUAD_LEFT_EDGE {
        Some(AnchorId::EdgeMid(QUAD_LEFT_EDGE_MIDPOINT))
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use bevy_ecs::bundle::Bundle;
    use bevy_ecs::entity::Entity;
    use bevy_ecs::schedule::ApplyDeferred;
    use bevy_ecs::schedule::IntoScheduleConfigs;
    use bevy_ecs::schedule::Schedule;
    use bevy_ecs::world::World;
    use bevy_math::Quat;
    use bevy_math::Vec3;
    use bevy_transform::prelude::Transform;

    use super::Accordion;
    use super::ArrangementMembers;
    use super::Coil;
    use super::Member;
    use super::MemberIndex;
    use super::PendingMemberPlacement;
    use super::QuadTiling;
    use super::Strip;
    use super::apply_member_placements;
    use super::assign_member_indices;
    use super::drive_arrangement_hinges;
    use crate::AnchoredTo;
    use crate::FoldAngles;
    use crate::Hinge;
    use crate::hinge_to_pose;
    use crate::resolve;
    use crate::resolve_anchors;

    const ASSERT_EPSILON: f32 = 1e-4;
    const COIL_WORLD_ANGLE_SCALE: f32 = 4.0;
    const EXPECTED_HINGED_MEMBERS: usize = 4;
    const FOLD: f32 = 1.0;
    const FOLD_ANGLE: f32 = core::f32::consts::FRAC_PI_2;
    const HALF_FOLD: f32 = 0.5;
    const MEMBER_ONE_INDEX: usize = 1;
    const MEMBER_THREE_INDEX: usize = 3;
    const QUAD_HEIGHT: f32 = 1.0;
    const ROOT_INDEX: usize = 0;

    #[test]
    fn five_quad_accordion_writes_alternating_hinge_angles() {
        let mut world = resolve::world_with_diagnostics();
        let root = spawn_arrangement_root(
            &mut world,
            Accordion {
                fold: FOLD,
                lean: FOLD_ANGLE,
            },
        );
        let members = spawn_members(&mut world, root, EXPECTED_HINGED_MEMBERS);
        run_arrangement_schedule(&mut world);

        let angles = hinge_angles(&world, &members);
        assert_close(angles[0], FOLD_ANGLE);
        assert_close(angles[1], -FOLD_ANGLE);
        assert_close(angles[2], FOLD_ANGLE);
        assert_close(angles[3], -FOLD_ANGLE);
    }

    #[test]
    fn five_quad_coil_accumulates_world_rotation() {
        let mut world = resolve::world_with_diagnostics();
        let root = spawn_arrangement_root(
            &mut world,
            Coil {
                fold: FOLD,
                lean: FOLD_ANGLE,
            },
        );
        let members = spawn_members(&mut world, root, EXPECTED_HINGED_MEMBERS);
        run_arrangement_schedule(&mut world);

        let last = members[EXPECTED_HINGED_MEMBERS - 1];
        assert_close_quat(
            transform(&world, last).rotation,
            Quat::from_rotation_x(FOLD_ANGLE * COIL_WORLD_ANGLE_SCALE),
        );
    }

    #[test]
    fn member_spawned_mid_fold_gets_live_hinge_angle() {
        let mut world = resolve::world_with_diagnostics();
        let root = spawn_arrangement_root(
            &mut world,
            Accordion {
                fold: HALF_FOLD,
                lean: FOLD_ANGLE,
            },
        );
        run_arrangement_schedule(&mut world);

        let member = spawn_member(&mut world, root);
        run_arrangement_schedule(&mut world);

        let angle = world.get::<Hinge>(member).map_or(0.0, |hinge| hinge.angle);
        assert_close(angle, FOLD_ANGLE * HALF_FOLD);
    }

    #[test]
    fn fold_angles_transfer_hinge_ownership_until_removed() {
        let mut world = World::new();
        let arrangement = world
            .spawn((
                Accordion {
                    fold: FOLD,
                    lean: FOLD_ANGLE,
                },
                QuadTiling,
            ))
            .id();
        let member = world
            .spawn((
                Member { arrangement },
                MemberIndex {
                    index: MEMBER_ONE_INDEX,
                },
                Hinge {
                    edge:  super::QUAD_TOP_EDGE,
                    angle: HALF_FOLD,
                },
                FoldAngles {
                    unfolded: 0.0,
                    folded:   HALF_FOLD,
                },
            ))
            .id();
        let mut schedule = Schedule::default();
        schedule.add_systems(drive_arrangement_hinges::<QuadTiling>);

        schedule.run(&mut world);
        assert_close(
            world.get::<Hinge>(member).map_or(0.0, |hinge| hinge.angle),
            HALF_FOLD,
        );

        world.entity_mut(member).remove::<FoldAngles>();
        schedule.run(&mut world);
        assert_close(
            world.get::<Hinge>(member).map_or(0.0, |hinge| hinge.angle),
            FOLD_ANGLE,
        );
    }

    #[test]
    fn non_contiguous_member_indices_use_previous_live_member() {
        let mut world = resolve::world_with_diagnostics();
        let root = resolve::spawn_quad(&mut world, Transform::default());
        world.entity_mut(root).insert((Strip, QuadTiling));
        let first = resolve::spawn_quad(&mut world, Transform::default());
        let third = resolve::spawn_quad(&mut world, Transform::default());
        world.entity_mut(root).insert(ArrangementMembers {
            entities: vec![first, third],
        });
        world.entity_mut(first).insert((
            Member { arrangement: root },
            MemberIndex {
                index: MEMBER_ONE_INDEX,
            },
            PendingMemberPlacement,
        ));
        world.entity_mut(third).insert((
            Member { arrangement: root },
            MemberIndex {
                index: MEMBER_THREE_INDEX,
            },
            PendingMemberPlacement,
        ));
        run_arrangement_schedule(&mut world);

        assert_eq!(
            world.get::<AnchoredTo>(third).map(AnchoredTo::target),
            Some(first),
        );
    }

    #[test]
    fn strip_rest_layout_matches_hand_computed_seats() {
        let mut world = resolve::world_with_diagnostics();
        let root = resolve::spawn_quad(&mut world, Transform::default());
        world.entity_mut(root).insert((Strip, QuadTiling));
        let members = spawn_members(&mut world, root, EXPECTED_HINGED_MEMBERS);
        run_arrangement_schedule(&mut world);

        assert_close_vec3(transform(&world, root).translation, Vec3::ZERO);
        for (offset, entity) in members.into_iter().enumerate() {
            let index = offset + 1;
            assert_close_vec3(
                transform(&world, entity).translation,
                expected_strip_translation(index),
            );
        }
    }

    fn run_arrangement_schedule(world: &mut World) {
        let mut schedule = Schedule::default();
        schedule.add_systems(
            (
                assign_member_indices,
                ApplyDeferred,
                apply_member_placements::<QuadTiling>,
                ApplyDeferred,
                drive_arrangement_hinges::<QuadTiling>,
                hinge_to_pose,
                resolve_anchors,
            )
                .chain(),
        );
        schedule.run(world);
    }

    fn spawn_arrangement_root(world: &mut World, arrangement: impl Bundle) -> Entity {
        let root = resolve::spawn_quad(world, Transform::default());
        world.entity_mut(root).insert((arrangement, QuadTiling));
        root
    }

    fn spawn_members(world: &mut World, arrangement: Entity, count: usize) -> Vec<Entity> {
        (0..count)
            .map(|_| spawn_member(world, arrangement))
            .collect()
    }

    fn spawn_member(world: &mut World, arrangement: Entity) -> Entity {
        let member = resolve::spawn_quad(world, Transform::default());
        world.entity_mut(member).insert(Member { arrangement });
        member
    }

    fn hinge_angles(world: &World, members: &[Entity]) -> Vec<f32> {
        members
            .iter()
            .filter_map(|entity| world.get::<Hinge>(*entity))
            .map(|hinge| hinge.angle)
            .collect()
    }

    fn transform(world: &World, entity: Entity) -> Transform {
        world.get::<Transform>(entity).copied().unwrap_or_default()
    }

    fn expected_strip_translation(index: usize) -> Vec3 {
        let mut translation = Vec3::ZERO;
        for _ in ROOT_INDEX..index {
            translation.y -= QUAD_HEIGHT;
        }
        translation
    }

    fn assert_close(actual: f32, expected: f32) {
        assert!(
            (actual - expected).abs() <= ASSERT_EPSILON,
            "actual {actual}, expected {expected}",
        );
    }

    fn assert_close_vec3(actual: Vec3, expected: Vec3) {
        assert!(
            actual.abs_diff_eq(expected, ASSERT_EPSILON),
            "actual {actual:?}, expected {expected:?}",
        );
    }

    fn assert_close_quat(actual: Quat, expected: Quat) {
        assert!(
            actual.abs_diff_eq(expected, ASSERT_EPSILON)
                || actual.abs_diff_eq(-expected, ASSERT_EPSILON),
            "actual {actual:?}, expected {expected:?}",
        );
    }
}
