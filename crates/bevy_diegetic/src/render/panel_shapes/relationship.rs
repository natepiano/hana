//! The panel↔primitive relationship types for the panel-shape path.
//!
//! These components define the typed traversal index. The panel-shape producer
//! does not consume them yet — it routes through its current batch input path.

use std::ops::Deref;

use bevy::prelude::*;

/// Marker on a panel-shape primitive entity owned by a diegetic panel.
///
/// The marker identifies entities that participate in the
/// [`PanelShapeOf`] / [`PanelShapes`] traversal index. It does not carry batch
/// data or material-slot ownership.
#[derive(Component, Clone, Copy, Debug, Default, Eq, PartialEq, Reflect)]
#[reflect(Component, PartialEq, Debug, FromWorld, Default, Clone)]
pub(super) struct PanelShape;

/// Relationship source on each panel-shape primitive, pointing at its panel.
///
/// Mirrors [`ChildOf`]: a public entity field plus a [`panel`](Self::panel)
/// accessor. Bevy maintains the matching [`PanelShapes`] set on the panel as
/// primitives carrying this component spawn and despawn.
#[derive(Component, Clone, Copy, Debug, Eq, PartialEq, Reflect)]
#[reflect(Component, PartialEq, Debug, FromWorld, Clone)]
#[relationship(relationship_target = PanelShapes)]
pub(super) struct PanelShapeOf(#[entities] pub(super) Entity);

impl PanelShapeOf {
    /// The panel entity this primitive belongs to.
    #[must_use]
    #[expect(
        dead_code,
        reason = "the panel-shape producer does not consume this relationship accessor yet"
    )]
    pub(super) const fn panel(self) -> Entity { self.0 }
}

impl FromWorld for PanelShapeOf {
    fn from_world(_world: &mut World) -> Self { Self(Entity::PLACEHOLDER) }
}

/// Relationship target on a panel: the set of its panel-shape primitives.
///
/// Bevy maintains this set from [`PanelShapeOf`] components. The target is a
/// traversal index only; the normal hierarchy remains responsible for
/// transform propagation and linked despawn behavior.
#[derive(Component, Default, Debug, Eq, PartialEq, Reflect)]
#[reflect(Component, FromWorld, Default)]
#[relationship_target(relationship = PanelShapeOf)]
pub(super) struct PanelShapes(Vec<Entity>);

impl Deref for PanelShapes {
    type Target = [Entity];

    fn deref(&self) -> &Self::Target { &self.0 }
}

/// Material-source identity for a panel-shape primitive, keyed by the source
/// entity rather than a render key.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
#[expect(
    dead_code,
    reason = "the panel-shape producer does not consume this material source key yet"
)]
pub(super) struct PanelShapeMaterialSourceKey {
    /// Panel-shape primitive whose material source is being projected.
    pub(super) shape: Entity,
}
