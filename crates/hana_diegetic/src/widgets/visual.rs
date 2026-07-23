//! Retained widget visual slots and state-only presentation overrides.
//!
//! A visual slot is a stable private id authored on an ordinary layout
//! element inside a widget subtree. Layout output records each slot's
//! element index on the owning `ComputedWidgetRecord`; reify copies those
//! references onto the widget entity as [`WidgetVisualSlots`]. Widget state
//! writes [`WidgetVisualOverrides`], and [`dispatch_visual_overrides`]
//! resolves the changed slot set into the [`VisualOverrideIndex`] the four
//! retained-batch routes read: `route_sdf_batch_records`,
//! `route_image_batch_records`, `update_panel_text_batches`, and
//! `reconcile_panel_line_batches`.
//!
//! An override never rewrites `DiegeticPanel`, regenerates the `LayoutTree`,
//! changes `ComputedDiegeticPanel`, or runs geometry solving: it is applied
//! while the routes rebuild retained batch records, so authored data stays
//! untouched and clearing the override restores the authored appearance.
//! Overrides patch records that layout already emitted; they never create a
//! record for an unauthored fill, border, image, text, or panel-line role.

use std::collections::HashMap;

use bevy::prelude::*;

use super::PanelWidget;
use super::WidgetOf;

/// Stable private id for one widget-owned visual slot.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(crate) struct VisualSlotId(u32);

impl VisualSlotId {
    /// Creates a slot id from a preset-chosen stable value in renderer tests.
    #[cfg(test)]
    #[must_use]
    pub(crate) const fn new(value: u32) -> Self { Self(value) }
}

/// One slot-to-record reference carried by a `ComputedWidgetRecord`.
///
/// The element index resolves to every retained record the slot element
/// authored: its SDF fill/border surface, image quad, text runs, and
/// panel-line groups all carry the same `LayoutTree` element index.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct ComputedVisualSlot {
    /// Authored stable slot id.
    pub slot:          VisualSlotId,
    /// Index of the slot element in the panel's `LayoutTree`.
    pub element_index: usize,
}

/// Reified slot-to-record references owned by one widget entity.
#[derive(Clone, Component, Debug, Default, PartialEq)]
pub(crate) struct WidgetVisualSlots {
    slots: Vec<ComputedVisualSlot>,
}

impl WidgetVisualSlots {
    #[must_use]
    pub(crate) const fn new(slots: Vec<ComputedVisualSlot>) -> Self { Self { slots } }

    /// Resolves a stable slot id to its current `LayoutTree` element index.
    #[must_use]
    pub(crate) fn element_index(&self, slot: VisualSlotId) -> Option<usize> {
        self.slots
            .iter()
            .find(|computed| computed.slot == slot)
            .map(|computed| computed.element_index)
    }
}

/// State-only presentation override for one visual slot.
///
/// `color` recolors the slot's authored fill, border, image tint, text, or
/// panel-line records without changing batch routing. `offset` translates
/// the slot's SDF, image, text, and panel-line records in the panel-local XY
/// plane while preserving authored draw depth. `material`
/// replaces the SDF, text, or panel-line source material and re-keys the
/// record when the replacement changes pipeline or resource compatibility.
/// `texture` replaces an image record's sampled texture and re-keys it to
/// the destination `ImageBatchKey`.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct VisualSlotOverride {
    /// Replacement color for authored fill/tint/text/line color.
    pub color:    Option<Color>,
    /// Panel-local XY translation added to retained record transforms.
    pub offset:   Option<Vec2>,
    /// Replacement source material for SDF, text, and panel-line records.
    pub material: Option<Handle<StandardMaterial>>,
    /// Replacement sampled texture for image records.
    pub texture:  Option<Handle<Image>>,
}

/// Fluent construction helpers for retained-renderer tests.
#[cfg(test)]
impl VisualSlotOverride {
    #[must_use]
    pub(crate) const fn with_color(mut self, color: Color) -> Self {
        self.color = Some(color);
        self
    }

    #[must_use]
    pub(crate) const fn with_offset(mut self, offset: Vec2) -> Self {
        self.offset = Some(offset);
        self
    }

    #[must_use]
    pub(crate) fn with_material(mut self, material: Handle<StandardMaterial>) -> Self {
        self.material = Some(material);
        self
    }

    #[must_use]
    pub(crate) fn with_texture(mut self, texture: Handle<Image>) -> Self {
        self.texture = Some(texture);
        self
    }
}

/// Changed-only override authoring owned by one widget entity.
#[derive(Clone, Component, Debug, Default, PartialEq)]
pub(crate) struct WidgetVisualOverrides {
    overrides: Vec<(VisualSlotId, VisualSlotOverride)>,
}

impl WidgetVisualOverrides {
    /// Sets or replaces the override for `slot`; an equal stored value is
    /// left untouched.
    ///
    /// The skip cannot suppress Bevy change detection on its own: reaching
    /// `set` through `Mut<WidgetVisualOverrides>` already marked the
    /// component changed, so [`dispatch_visual_overrides`] re-resolves the
    /// widget's index entries. The repeated-identical no-op guarantee lives
    /// at the retained renderer level — every route rebuilds the same record
    /// values and the batch stores compare before dirtying, so no GPU buffer
    /// re-uploads. Production writers must compare immutably before taking a
    /// mutable component reference, keeping unchanged frames out of
    /// `Changed<WidgetVisualOverrides>` entirely.
    ///
    /// Test-only mutation hook for retained-renderer coverage.
    #[cfg(test)]
    pub(crate) fn set(&mut self, slot: VisualSlotId, value: VisualSlotOverride) {
        match self.overrides.iter_mut().find(|(id, _)| *id == slot) {
            Some((_, current)) => {
                if *current != value {
                    *current = value;
                }
            },
            None => self.overrides.push((slot, value)),
        }
    }

    /// Removes the override for `slot`, restoring authored presentation.
    ///
    /// Test-only mutation hook for retained-renderer coverage.
    #[cfg(test)]
    pub(crate) fn clear(&mut self, slot: VisualSlotId) {
        self.overrides.retain(|(id, _)| *id != slot);
    }

    fn iter(&self) -> impl Iterator<Item = (VisualSlotId, &VisualSlotOverride)> {
        self.overrides.iter().map(|(slot, value)| (*slot, value))
    }
}

/// Resolved override lookup consumed by the retained-batch route systems.
///
/// Keys are `(panel entity, LayoutTree element index)` — the identity every
/// route already has in hand while rebuilding a record.
#[derive(Default, Resource)]
pub(crate) struct VisualOverrideIndex {
    by_record: HashMap<(Entity, usize), VisualSlotOverride>,
    by_widget: HashMap<Entity, Vec<(Entity, usize)>>,
}

impl VisualOverrideIndex {
    /// Current override for one panel element's retained records.
    #[must_use]
    pub(crate) fn get(&self, panel: Entity, element_index: usize) -> Option<&VisualSlotOverride> {
        self.by_record.get(&(panel, element_index))
    }

    fn insert_widget(
        &mut self,
        widget: Entity,
        entries: Vec<((Entity, usize), VisualSlotOverride)>,
    ) {
        if entries.is_empty() {
            return;
        }
        let mut keys = Vec::with_capacity(entries.len());
        for (key, value) in entries {
            keys.push(key);
            self.by_record.insert(key, value);
        }
        self.by_widget.insert(widget, keys);
    }

    fn remove_widget(&mut self, widget: Entity) {
        let Some(keys) = self.by_widget.remove(&widget) else {
            return;
        };
        for key in keys {
            self.by_record.remove(&key);
        }
    }
}

/// Resolves changed widget overrides into the [`VisualOverrideIndex`].
///
/// Runs after `WidgetSystems::ReifyCommandsApplied` so slot references
/// attached by this frame's reify are visible; the `PostUpdate` batch routes
/// read the index later the same frame.
///
/// Removal of stale keys happens for every changed and removed widget before
/// any insertion. A structural tree edit renumbers element indices, so one
/// widget's previous `(panel, element_index)` key can equal another changed
/// widget's current key; removing after inserting would delete that widget's
/// fresh [`VisualOverrideIndex`] entry.
pub(crate) fn dispatch_visual_overrides(
    changed_widgets: Query<
        (
            Entity,
            &WidgetOf,
            &WidgetVisualSlots,
            Option<&WidgetVisualOverrides>,
        ),
        Or<(Changed<WidgetVisualOverrides>, Changed<WidgetVisualSlots>)>,
    >,
    live_overrides: Query<(), With<WidgetVisualOverrides>>,
    mut removed_widgets: RemovedComponents<PanelWidget>,
    mut removed_overrides: RemovedComponents<WidgetVisualOverrides>,
    mut index: ResMut<VisualOverrideIndex>,
) {
    for (widget, ..) in &changed_widgets {
        index.remove_widget(widget);
    }
    for widget in removed_overrides.read() {
        if live_overrides.get(widget).is_err() {
            index.remove_widget(widget);
        }
    }
    for widget in removed_widgets.read() {
        index.remove_widget(widget);
    }
    for (widget, widget_of, slots, overrides) in &changed_widgets {
        let entries = overrides.map_or_else(Vec::new, |overrides| {
            overrides
                .iter()
                .filter_map(|(slot, value)| {
                    slots
                        .element_index(slot)
                        .map(|element_index| ((widget_of.panel(), element_index), value.clone()))
                })
                .collect()
        });
        index.insert_widget(widget, entries);
    }
}

#[cfg(test)]
#[allow(
    clippy::expect_used,
    reason = "tests should panic on unexpected values"
)]
mod tests {
    use bevy::prelude::*;

    use super::ComputedVisualSlot;
    use super::VisualOverrideIndex;
    use super::VisualSlotId;
    use super::VisualSlotOverride;
    use super::WidgetVisualOverrides;
    use super::WidgetVisualSlots;
    use crate::PanelElementId;
    use crate::widgets::PanelWidget;
    use crate::widgets::WidgetOf;

    const SLOT: VisualSlotId = VisualSlotId::new(7);
    const SLOT_ELEMENT_INDEX: usize = 3;
    const OVERRIDE_COLOR: Color = Color::srgb(0.9, 0.1, 0.2);
    const PEER_ELEMENT_INDEX: usize = 5;
    const PEER_OVERRIDE_COLOR: Color = Color::srgb(0.2, 0.8, 0.9);

    fn dispatch_app() -> App {
        let mut app = App::new();
        app.init_resource::<VisualOverrideIndex>()
            .add_systems(Update, super::dispatch_visual_overrides);
        app
    }

    fn spawn_slotted_widget(app: &mut App, panel: Entity) -> Entity {
        app.world_mut()
            .spawn((
                PanelWidget::new(PanelElementId::named("styled")),
                WidgetOf::new(panel),
                WidgetVisualSlots::new(vec![ComputedVisualSlot {
                    slot:          SLOT,
                    element_index: SLOT_ELEMENT_INDEX,
                }]),
            ))
            .id()
    }

    fn spawn_overridden_widget(
        app: &mut App,
        panel: Entity,
        name: &str,
        element_index: usize,
        color: Color,
    ) -> Entity {
        let mut overrides = WidgetVisualOverrides::default();
        overrides.set(SLOT, VisualSlotOverride::default().with_color(color));
        app.world_mut()
            .spawn((
                PanelWidget::new(PanelElementId::named(name)),
                WidgetOf::new(panel),
                WidgetVisualSlots::new(vec![ComputedVisualSlot {
                    slot: SLOT,
                    element_index,
                }]),
                overrides,
            ))
            .id()
    }

    fn indexed_color(app: &App, panel: Entity) -> Option<Color> {
        indexed_color_at(app, panel, SLOT_ELEMENT_INDEX)
    }

    fn indexed_color_at(app: &App, panel: Entity, element_index: usize) -> Option<Color> {
        app.world()
            .resource::<VisualOverrideIndex>()
            .get(panel, element_index)
            .and_then(|value| value.color)
    }

    #[test]
    fn dispatch_indexes_override_by_panel_and_element_index() {
        let mut app = dispatch_app();
        let panel = app.world_mut().spawn_empty().id();
        let widget = spawn_slotted_widget(&mut app, panel);
        app.update();
        assert_eq!(indexed_color(&app, panel), None);

        let mut overrides = WidgetVisualOverrides::default();
        overrides.set(
            SLOT,
            VisualSlotOverride::default().with_color(OVERRIDE_COLOR),
        );
        app.world_mut().entity_mut(widget).insert(overrides);
        app.update();

        assert_eq!(indexed_color(&app, panel), Some(OVERRIDE_COLOR));
        assert!(
            app.world()
                .resource::<VisualOverrideIndex>()
                .get(panel, SLOT_ELEMENT_INDEX + 1)
                .is_none(),
            "unrelated element indices must stay unindexed",
        );
    }

    #[test]
    fn unknown_slot_ids_index_nothing() {
        let mut app = dispatch_app();
        let panel = app.world_mut().spawn_empty().id();
        let widget = spawn_slotted_widget(&mut app, panel);
        let mut overrides = WidgetVisualOverrides::default();
        overrides.set(
            VisualSlotId::new(99),
            VisualSlotOverride::default().with_color(OVERRIDE_COLOR),
        );
        app.world_mut().entity_mut(widget).insert(overrides);
        app.update();

        assert!(
            app.world()
                .resource::<VisualOverrideIndex>()
                .get(panel, SLOT_ELEMENT_INDEX)
                .is_none()
        );
    }

    #[test]
    fn clearing_and_removal_retire_index_entries() {
        let mut app = dispatch_app();
        let panel = app.world_mut().spawn_empty().id();
        let widget = spawn_slotted_widget(&mut app, panel);
        let mut overrides = WidgetVisualOverrides::default();
        overrides.set(
            SLOT,
            VisualSlotOverride::default().with_color(OVERRIDE_COLOR),
        );
        app.world_mut().entity_mut(widget).insert(overrides);
        app.update();
        assert_eq!(indexed_color(&app, panel), Some(OVERRIDE_COLOR));

        let mut overrides = app
            .world_mut()
            .get_mut::<WidgetVisualOverrides>(widget)
            .expect("widget should keep its override component");
        overrides.clear(SLOT);
        app.update();
        assert_eq!(indexed_color(&app, panel), None);

        let mut overrides = app
            .world_mut()
            .get_mut::<WidgetVisualOverrides>(widget)
            .expect("widget should keep its override component");
        overrides.set(
            SLOT,
            VisualSlotOverride::default().with_color(OVERRIDE_COLOR),
        );
        app.update();
        assert_eq!(indexed_color(&app, panel), Some(OVERRIDE_COLOR));

        app.world_mut()
            .entity_mut(widget)
            .remove::<WidgetVisualOverrides>();
        app.update();
        assert_eq!(indexed_color(&app, panel), None);

        let mut overrides = WidgetVisualOverrides::default();
        overrides.set(
            SLOT,
            VisualSlotOverride::default().with_color(OVERRIDE_COLOR),
        );
        app.world_mut().entity_mut(widget).insert(overrides);
        app.update();
        assert_eq!(indexed_color(&app, panel), Some(OVERRIDE_COLOR));

        app.world_mut().entity_mut(widget).despawn();
        app.update();
        assert_eq!(indexed_color(&app, panel), None);
    }

    #[test]
    fn removed_widget_stale_key_keeps_renumbered_widget_entry() {
        let mut app = dispatch_app();
        let panel = app.world_mut().spawn_empty().id();
        let removed = spawn_overridden_widget(
            &mut app,
            panel,
            "removed",
            SLOT_ELEMENT_INDEX,
            OVERRIDE_COLOR,
        );
        let renumbered = spawn_overridden_widget(
            &mut app,
            panel,
            "renumbered",
            PEER_ELEMENT_INDEX,
            PEER_OVERRIDE_COLOR,
        );
        app.update();
        assert_eq!(
            indexed_color_at(&app, panel, SLOT_ELEMENT_INDEX),
            Some(OVERRIDE_COLOR),
        );
        assert_eq!(
            indexed_color_at(&app, panel, PEER_ELEMENT_INDEX),
            Some(PEER_OVERRIDE_COLOR),
        );

        // One structural edit removes a widget and renumbers the survivor
        // onto the removed widget's old element index in the same frame.
        app.world_mut().entity_mut(removed).despawn();
        app.world_mut()
            .entity_mut(renumbered)
            .insert(WidgetVisualSlots::new(vec![ComputedVisualSlot {
                slot:          SLOT,
                element_index: SLOT_ELEMENT_INDEX,
            }]));
        app.update();

        assert_eq!(
            indexed_color_at(&app, panel, SLOT_ELEMENT_INDEX),
            Some(PEER_OVERRIDE_COLOR),
            "the removed widget's stale key must not delete the renumbered widget's entry",
        );
        assert_eq!(
            indexed_color_at(&app, panel, PEER_ELEMENT_INDEX),
            None,
            "the renumbered widget's old key must retire",
        );
    }

    #[test]
    fn overlapping_renumber_keeps_both_changed_widget_entries() {
        let mut app = dispatch_app();
        let panel = app.world_mut().spawn_empty().id();
        let first =
            spawn_overridden_widget(&mut app, panel, "first", SLOT_ELEMENT_INDEX, OVERRIDE_COLOR);
        let second = spawn_overridden_widget(
            &mut app,
            panel,
            "second",
            PEER_ELEMENT_INDEX,
            PEER_OVERRIDE_COLOR,
        );
        app.update();

        // One structural edit swaps the two widgets' element indices, so each
        // widget's old key equals the other's current key and single-pass
        // removal would delete a fresh entry in either iteration order.
        app.world_mut()
            .entity_mut(first)
            .insert(WidgetVisualSlots::new(vec![ComputedVisualSlot {
                slot:          SLOT,
                element_index: PEER_ELEMENT_INDEX,
            }]));
        app.world_mut()
            .entity_mut(second)
            .insert(WidgetVisualSlots::new(vec![ComputedVisualSlot {
                slot:          SLOT,
                element_index: SLOT_ELEMENT_INDEX,
            }]));
        app.update();

        assert_eq!(
            indexed_color_at(&app, panel, PEER_ELEMENT_INDEX),
            Some(OVERRIDE_COLOR),
        );
        assert_eq!(
            indexed_color_at(&app, panel, SLOT_ELEMENT_INDEX),
            Some(PEER_OVERRIDE_COLOR),
        );
    }
}
