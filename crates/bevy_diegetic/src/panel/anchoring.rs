//! Panel-to-panel anchoring relationship types.

use std::ops::Deref;

use bevy::prelude::*;

use crate::layout::Anchor;
use crate::layout::Dimension;
use crate::layout::Unit;

/// Relationship source that pins one panel anchor point to another panel.
#[derive(Component, Clone, Copy, Debug, PartialEq, Reflect)]
#[component(immutable)]
#[reflect(PartialEq, Debug, FromWorld, Clone)]
#[relationship(relationship_target = PanelsAnchoredHere)]
pub struct AnchoredToPanel {
    #[relationship]
    #[entities]
    #[reflect(ignore, default = "placeholder_entity")]
    target:            Entity,
    /// Anchor point on the source panel.
    pub source_anchor: Anchor,
    /// Anchor point on the target panel.
    pub target_anchor: Anchor,
    /// Offset from the resolved target point.
    pub offset:        PanelAnchorOffset,
}

impl AnchoredToPanel {
    /// Creates a relationship from the source panel to `target`.
    #[must_use]
    pub const fn new(target: Entity, source_anchor: Anchor, target_anchor: Anchor) -> Self {
        Self {
            target,
            source_anchor,
            target_anchor,
            offset: PanelAnchorOffset::ZERO,
        }
    }

    /// Sets the offset from the resolved target anchor point.
    #[must_use]
    pub const fn with_offset(mut self, offset: PanelAnchorOffset) -> Self {
        self.offset = offset;
        self
    }

    /// Target panel entity.
    #[must_use]
    pub const fn target(&self) -> Entity { self.target }

    /// Returns a copy that points at `target`.
    #[must_use]
    pub const fn retargeted(mut self, target: Entity) -> Self {
        self.target = target;
        self
    }
}

impl FromWorld for AnchoredToPanel {
    fn from_world(_world: &mut World) -> Self {
        Self::new(Entity::PLACEHOLDER, Anchor::Center, Anchor::Center)
    }
}

const fn placeholder_entity() -> Entity { Entity::PLACEHOLDER }

/// Offset from a target panel anchor point.
///
/// Coordinates are authored in panel-local layout space: positive x moves
/// right, positive y moves down. Bare `f32` values resolve against the target
/// panel's layout unit; [`Px`](crate::Px), [`Mm`](crate::Mm),
/// [`Pt`](crate::Pt), and [`In`](crate::In) carry explicit units.
#[derive(Clone, Copy, Debug, Default, PartialEq, Reflect)]
#[reflect(PartialEq, Debug, Default)]
pub struct PanelAnchorOffset {
    x: Dimension,
    y: Dimension,
}

impl PanelAnchorOffset {
    /// Zero offset.
    pub const ZERO: Self = Self {
        x: Dimension {
            value: 0.0,
            unit:  None,
        },
        y: Dimension {
            value: 0.0,
            unit:  None,
        },
    };

    /// Creates an offset from two authored dimensions.
    #[must_use]
    pub fn new(x: impl Into<Dimension>, y: impl Into<Dimension>) -> Self {
        Self {
            x: x.into(),
            y: y.into(),
        }
    }

    /// Horizontal offset dimension.
    #[must_use]
    pub const fn x(self) -> Dimension { self.x }

    /// Vertical offset dimension.
    #[must_use]
    pub const fn y(self) -> Dimension { self.y }

    pub(crate) fn to_layout_units(self, layout_unit: Unit) -> Vec2 {
        let layout_to_points = layout_unit.to_points();
        Vec2::new(
            self.x.to_points(layout_to_points) / layout_to_points,
            self.y.to_points(layout_to_points) / layout_to_points,
        )
    }
}

/// Reverse relationship target: panels anchored to this panel.
#[derive(Component, Default, Debug, Eq, PartialEq, Reflect)]
#[reflect(FromWorld, Default)]
#[relationship_target(relationship = AnchoredToPanel)]
pub struct PanelsAnchoredHere(Vec<Entity>);

impl PanelsAnchoredHere {
    /// Iterates over source panel entities.
    pub fn iter(&self) -> impl Iterator<Item = Entity> + '_ { self.0.iter().copied() }

    /// Number of source panels currently pointing here.
    #[must_use]
    pub const fn len(&self) -> usize { self.0.len() }

    /// Whether no source panels point here.
    #[must_use]
    pub const fn is_empty(&self) -> bool { self.0.is_empty() }
}

impl Deref for PanelsAnchoredHere {
    type Target = [Entity];

    fn deref(&self) -> &Self::Target { &self.0 }
}

/// Resolver-owned screen position override for a panel's configured anchor.
#[derive(Component, Clone, Copy, Debug, Default, PartialEq)]
pub(crate) struct ResolvedScreenPanelPosition {
    pub(crate) anchor_position: Option<Vec2>,
}

#[cfg(test)]
#[allow(
    clippy::expect_used,
    reason = "tests should panic on unexpected values"
)]
mod tests {
    use std::any::TypeId;

    use bevy::ecs::reflect::ReflectComponent;
    use bevy::ecs::relationship::Relationship;
    use bevy::prelude::*;

    use super::AnchoredToPanel;
    use super::PanelAnchorOffset;
    use super::PanelsAnchoredHere;
    use crate::HeadlessLayoutPlugin;
    use crate::layout::Anchor;

    fn reverse_targets(world: &World, target: Entity) -> Vec<Entity> {
        world
            .get::<PanelsAnchoredHere>(target)
            .map(|targets| targets.iter().collect())
            .unwrap_or_default()
    }

    #[test]
    fn insert_replace_and_remove_update_reverse_index() {
        let mut world = World::new();
        let target_a = world.spawn_empty().id();
        let target_b = world.spawn_empty().id();
        let source = world.spawn_empty().id();

        world.entity_mut(source).insert(AnchoredToPanel::new(
            target_a,
            Anchor::TopLeft,
            Anchor::BottomLeft,
        ));
        assert_eq!(reverse_targets(&world, target_a), vec![source]);
        assert!(reverse_targets(&world, target_b).is_empty());

        world.entity_mut(source).insert(
            AnchoredToPanel::new(target_a, Anchor::TopRight, Anchor::BottomRight)
                .retargeted(target_b),
        );
        assert!(reverse_targets(&world, target_a).is_empty());
        assert_eq!(reverse_targets(&world, target_b), vec![source]);

        world.entity_mut(source).remove::<AnchoredToPanel>();
        assert!(reverse_targets(&world, target_b).is_empty());
    }

    #[test]
    fn despawning_target_detaches_without_despawning_dependent() {
        let mut world = World::new();
        let target = world.spawn_empty().id();
        let source = world
            .spawn(AnchoredToPanel::new(
                target,
                Anchor::TopLeft,
                Anchor::BottomLeft,
            ))
            .id();

        world.entity_mut(target).despawn();

        assert!(world.get_entity(source).is_ok());
        assert!(world.get::<AnchoredToPanel>(source).is_none());
    }

    #[test]
    fn relationship_from_uses_center_to_center_zero_offset_defaults() {
        let target = Entity::PLACEHOLDER;
        let derived = <AnchoredToPanel as Relationship>::from(target);
        let expected = AnchoredToPanel::new(target, Anchor::Center, Anchor::Center)
            .with_offset(PanelAnchorOffset::ZERO);

        assert_eq!(derived, expected);
    }

    #[test]
    fn relationship_types_are_registered_without_reflect_component_mutation() {
        let mut app = App::new();
        app.add_plugins(HeadlessLayoutPlugin);

        let registry = app.world().resource::<AppTypeRegistry>().read();
        let source_has_reflect_component = registry
            .get(TypeId::of::<AnchoredToPanel>())
            .expect("AnchoredToPanel is registered")
            .data::<ReflectComponent>()
            .is_some();
        let reverse_has_reflect_component = registry
            .get(TypeId::of::<PanelsAnchoredHere>())
            .expect("PanelsAnchoredHere is registered")
            .data::<ReflectComponent>()
            .is_some();
        let offset_has_reflect_component = registry
            .get(TypeId::of::<PanelAnchorOffset>())
            .expect("PanelAnchorOffset is registered")
            .data::<ReflectComponent>()
            .is_some();
        drop(registry);

        assert!(!source_has_reflect_component);
        assert!(!reverse_has_reflect_component);
        assert!(!offset_has_reflect_component);
    }

    #[test]
    fn anchored_to_panel_component_is_immutable() {
        let mut world = World::new();
        world.register_component::<AnchoredToPanel>();
        let component_id = world
            .components()
            .component_id::<AnchoredToPanel>()
            .expect("component registered");
        let component = world
            .components()
            .get_info(component_id)
            .expect("component info exists");

        assert!(!component.mutable());
    }
}
