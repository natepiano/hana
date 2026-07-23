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
use crate::layout::BoundingBox;

/// Stable private id for one widget-owned visual slot.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(crate) struct VisualSlotId(u32);

impl VisualSlotId {
    /// Root-surface slot authored by [`El::button`](crate::El::button) on the
    /// element carrying the widget. Button state presentation writes only this
    /// slot.
    pub(crate) const BUTTON_ROOT: Self = Self(u32::MAX);
    /// Root slot authored by [`El::slider`](crate::El::slider) on the element
    /// carrying the widget. Pointer projection reads its solved content box.
    pub(crate) const SLIDER_ROOT: Self = Self(u32::MAX - 1);
    /// Thumb slot authored by [`El::slider_thumb`](crate::El::slider_thumb) on
    /// one ordinary descendant of a slider. Value presentation reads its border
    /// box for the active-axis extent and solved authored center, then writes
    /// the slot's panel-local translation.
    pub(crate) const SLIDER_THUMB: Self = Self(u32::MAX - 2);

    /// Creates a slot id from a test-chosen stable value in renderer tests.
    #[cfg(test)]
    #[must_use]
    pub(crate) const fn new(value: u32) -> Self { Self(value) }
}

/// One slot-to-record reference carried by a `ComputedWidgetRecord`.
///
/// The element index resolves to every retained record the slot element
/// authored: its SDF fill/border surface, image quad, text runs, and
/// panel-line groups all carry the same `LayoutTree` element index.
/// `border_box` and `content_box` carry the slot element's solved outer bounds
/// and its padding/border-excluded interior, both in panel-layout coordinates,
/// so slider pointer projection reads the live content box without inspecting
/// retained render batches.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct ComputedVisualSlot {
    /// Authored stable slot id.
    pub slot:          VisualSlotId,
    /// Index of the slot element in the panel's `LayoutTree`.
    pub element_index: usize,
    /// Solved border box of the slot element in panel-layout coordinates.
    pub border_box:    BoundingBox,
    /// Solved padding/border-excluded content box of the slot element.
    pub content_box:   BoundingBox,
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

    /// Returns the slot's solved padding/border-excluded content box.
    #[must_use]
    pub(crate) fn content_box(&self, slot: VisualSlotId) -> Option<BoundingBox> {
        self.slots
            .iter()
            .find(|computed| computed.slot == slot)
            .map(|computed| computed.content_box)
    }

    /// Returns the slot's solved border box.
    #[must_use]
    pub(crate) fn border_box(&self, slot: VisualSlotId) -> Option<BoundingBox> {
        self.slots
            .iter()
            .find(|computed| computed.slot == slot)
            .map(|computed| computed.border_box)
    }
}

/// State-only presentation override for one visual slot.
///
/// `color` recolors the slot's authored fill, border, image tint, text, or
/// panel-line records without changing batch routing. `fill_color` and
/// `border_color` recolor only the slot's SDF fill or border role and take
/// precedence over `color` for that role; image, text, and panel-line records
/// never read them. `offset` translates
/// the slot's SDF, image, text, and panel-line records in the panel-local
/// render frame — panel world units with Y increasing upward — while
/// preserving authored draw depth. `material`
/// replaces the SDF, text, or panel-line source material and re-keys the
/// record when the replacement changes pipeline or resource compatibility.
/// `texture` replaces an image record's sampled texture and re-keys it to
/// the destination `ImageBatchKey`.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct VisualSlotOverride {
    /// Replacement color for authored fill/tint/text/line color.
    pub color:        Option<Color>,
    /// Replacement color for the SDF fill role only.
    pub fill_color:   Option<Color>,
    /// Replacement color for the SDF border role only.
    pub border_color: Option<Color>,
    /// Panel-local render-frame XY translation added to retained record
    /// transforms: panel world units with Y increasing upward, distinct from
    /// the layout-point frame (Y increasing downward) the widget slot boxes
    /// use. Produce it from a layout-frame delta with
    /// [`layout_delta_to_render_offset`].
    pub offset:       Option<Vec2>,
    /// Replacement source material for SDF, text, and panel-line records.
    pub material:     Option<Handle<StandardMaterial>>,
    /// Replacement sampled texture for image records.
    pub texture:      Option<Handle<Image>>,
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
    pub(crate) const fn with_fill_color(mut self, color: Color) -> Self {
        self.fill_color = Some(color);
        self
    }

    #[must_use]
    pub(crate) const fn with_border_color(mut self, color: Color) -> Self {
        self.border_color = Some(color);
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
    /// Returns the stored override for `slot`.
    #[must_use]
    pub(crate) fn get(&self, slot: VisualSlotId) -> Option<&VisualSlotOverride> {
        self.overrides
            .iter()
            .find(|(id, _)| *id == slot)
            .map(|(_, value)| value)
    }

    /// Sets or replaces the override for `slot`; an equal stored value is
    /// left untouched.
    ///
    /// The skip cannot suppress Bevy change detection on its own: reaching
    /// `set` through `Mut<WidgetVisualOverrides>` already marked the
    /// component changed, so [`dispatch_visual_overrides`] re-resolves the
    /// widget's index entries. The repeated-identical no-op guarantee lives
    /// at the retained renderer level — every route rebuilds the same record
    /// values and the batch stores compare before dirtying, so no GPU buffer
    /// re-uploads. Production writers go through [`write_slot_override`],
    /// which compares immutably before taking a mutable component reference,
    /// keeping unchanged frames out of `Changed<WidgetVisualOverrides>`
    /// entirely.
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
    pub(crate) fn clear(&mut self, slot: VisualSlotId) {
        self.overrides.retain(|(id, _)| *id != slot);
    }

    fn iter(&self) -> impl Iterator<Item = (VisualSlotId, &VisualSlotOverride)> {
        self.overrides.iter().map(|(slot, value)| (*slot, value))
    }
}

/// Writes one widget slot's desired override, touching mutable state only for
/// a real change.
///
/// A `desired` equal to [`VisualSlotOverride::default`] clears the slot. The
/// current component is read immutably first: an equal stored value (or an
/// absent value when clearing) returns before any `Mut` borrow, so unchanged
/// frames never enter `Changed<WidgetVisualOverrides>`. The first insertion
/// on a widget without the component is queued through `commands` and becomes
/// visible after `WidgetSystems::PresentationCommandsApplied`.
pub(crate) fn write_slot_override(
    widget: Entity,
    slot: VisualSlotId,
    desired: VisualSlotOverride,
    overrides: &mut Query<&mut WidgetVisualOverrides>,
    commands: &mut Commands<'_, '_>,
) {
    let clear = desired == VisualSlotOverride::default();
    let Ok(current) = overrides.get(widget) else {
        if clear {
            return;
        }
        let mut component = WidgetVisualOverrides::default();
        component.set(slot, desired);
        commands.entity(widget).insert(component);
        return;
    };
    let unchanged = match current.get(slot) {
        Some(existing) => !clear && *existing == desired,
        None => clear,
    };
    if unchanged {
        return;
    }
    let Ok(mut current) = overrides.get_mut(widget) else {
        return;
    };
    if clear {
        current.clear(slot);
    } else {
        current.set(slot, desired);
    }
}

/// Converts a widget slot's layout-frame translation delta into the
/// panel-local render frame the retained routes add to record transforms.
///
/// `layout_delta` is a delta in layout points: X increases rightward and Y
/// increases downward. The returned offset is in the panel-local render frame
/// — panel world units with X unchanged and Y increasing upward — so the
/// layout Y axis is inverted and both axes scale by `points_to_world`, the
/// owning panel's
/// [`DiegeticPanel::points_to_world`](crate::DiegeticPanel::points_to_world)
/// factor. It is the single boundary that reconciles those two frames for a
/// [`VisualSlotOverride::offset`]; returns `None` when `layout_delta` is
/// non-finite or `points_to_world` is non-finite or non-positive, so a slot
/// whose owning panel scale is unavailable writes no manufactured offset.
pub(crate) fn layout_delta_to_render_offset(
    layout_delta: Vec2,
    points_to_world: f32,
) -> Option<Vec2> {
    if !layout_delta.is_finite() || !points_to_world.is_finite() || points_to_world <= 0.0 {
        return None;
    }
    Some(Vec2::new(
        layout_delta.x * points_to_world,
        -layout_delta.y * points_to_world,
    ))
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
/// Runs after `WidgetSystems::PresentationCommandsApplied`, so slot
/// references attached by this frame's reify and the button state writer's
/// first override insertion are both visible; the `PostUpdate` batch routes
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
    use crate::layout::BoundingBox;
    use crate::widgets::PanelWidget;
    use crate::widgets::WidgetOf;

    const SLOT: VisualSlotId = VisualSlotId::new(7);
    const SLOT_ELEMENT_INDEX: usize = 3;
    const OVERRIDE_COLOR: Color = Color::srgb(0.9, 0.1, 0.2);
    const PEER_ELEMENT_INDEX: usize = 5;
    const PEER_OVERRIDE_COLOR: Color = Color::srgb(0.2, 0.8, 0.9);

    fn computed_slot(slot: VisualSlotId, element_index: usize) -> ComputedVisualSlot {
        ComputedVisualSlot {
            slot,
            element_index,
            border_box: BoundingBox::default(),
            content_box: BoundingBox::default(),
        }
    }

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
                WidgetVisualSlots::new(vec![computed_slot(SLOT, SLOT_ELEMENT_INDEX)]),
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
                WidgetVisualSlots::new(vec![computed_slot(SLOT, element_index)]),
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
    fn render_offset_scales_and_inverts_the_layout_y_axis() {
        // A non-unit scale multiplies both axes and the layout Y axis (down)
        // maps to the render Y axis (up), so the sign of Y flips.
        let offset = super::layout_delta_to_render_offset(Vec2::new(4.0, 6.0), 0.25)
            .expect("finite delta and positive scale convert");
        assert!((offset.x - 1.0).abs() < 1e-6, "X scales without inverting");
        assert!((offset.y + 1.5).abs() < 1e-6, "Y scales and inverts");
    }

    #[test]
    fn render_offset_rejects_invalid_input_or_scale() {
        assert_eq!(
            super::layout_delta_to_render_offset(Vec2::new(f32::NAN, 1.0), 1.0),
            None,
            "a non-finite delta manufactures no offset",
        );
        for scale in [0.0, -1.0, f32::NAN, f32::INFINITY] {
            assert_eq!(
                super::layout_delta_to_render_offset(Vec2::new(1.0, 1.0), scale),
                None,
                "a non-positive or non-finite scale manufactures no offset",
            );
        }
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
            .insert(WidgetVisualSlots::new(vec![computed_slot(
                SLOT,
                SLOT_ELEMENT_INDEX,
            )]));
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
            .insert(WidgetVisualSlots::new(vec![computed_slot(
                SLOT,
                PEER_ELEMENT_INDEX,
            )]));
        app.world_mut()
            .entity_mut(second)
            .insert(WidgetVisualSlots::new(vec![computed_slot(
                SLOT,
                SLOT_ELEMENT_INDEX,
            )]));
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
