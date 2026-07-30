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

use core::mem::size_of;
use std::collections::HashMap;
use std::collections::HashSet;

use bevy::prelude::*;

use super::PanelWidget;
use super::StateAppearance;
use super::VisualElementCapabilities;
use super::WidgetKind;
use super::WidgetOf;
use crate::DiegeticPanel;
use crate::layout::BoundingBox;

/// Stable private id for one widget-owned visual slot.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(crate) struct VisualSlotId(u32);

impl VisualSlotId {
    /// Root-surface slot authored by [`El::button`](crate::El::button) on the
    /// element carrying the widget. Button state presentation writes this slot
    /// and resolved part-element overrides for owned descendant visual recipients.
    pub(crate) const BUTTON_ROOT: Self = Self(u32::MAX);
    /// Root slot authored by [`El::slider`](crate::El::slider) on the element
    /// carrying the widget. Pointer projection reads its solved content box.
    pub(crate) const SLIDER_ROOT: Self = Self(u32::MAX - 1);
    /// Thumb slot authored by [`El::slider_thumb`](crate::El::slider_thumb) on
    /// one ordinary descendant of a slider. Value presentation reads its border
    /// box for the active-axis extent and solved authored center, then writes
    /// the slot's panel-local translation.
    pub(crate) const SLIDER_THUMB: Self = Self(u32::MAX - 2);
    /// Root-surface slot authored by [`El::editable_field`](crate::El::editable_field).
    /// Visible keyboard-focus presentation writes this slot and resolved part-element
    /// overrides for owned descendant visual recipients.
    pub(crate) const EDITABLE_ROOT: Self = Self(u32::MAX - 3);

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
    slots:            Vec<ComputedVisualSlot>,
    elements:         Vec<(usize, VisualElementCapabilities)>,
    part_appearances: Vec<(usize, StateAppearance)>,
}

impl WidgetVisualSlots {
    #[must_use]
    pub(crate) const fn new(slots: Vec<ComputedVisualSlot>) -> Self {
        Self {
            slots,
            elements: Vec::new(),
            part_appearances: Vec::new(),
        }
    }

    #[must_use]
    pub(crate) fn with_elements(
        mut self,
        elements: Vec<(usize, VisualElementCapabilities)>,
    ) -> Self {
        self.elements = elements;
        self
    }

    #[must_use]
    pub(crate) fn with_part_appearances(
        mut self,
        part_appearances: Vec<(usize, StateAppearance)>,
    ) -> Self {
        self.part_appearances = part_appearances;
        self
    }

    /// Returns every retained-record recipient owned by the widget declaration.
    #[must_use]
    pub(crate) fn elements(&self) -> &[(usize, VisualElementCapabilities)] { &self.elements }

    /// Returns every descendant with an explicitly authored state appearance.
    #[must_use]
    pub(crate) fn part_appearances(&self) -> &[(usize, StateAppearance)] { &self.part_appearances }

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
/// `color` recolors the slot's authored fill, border, or image tint without
/// changing batch routing. `text_color` and `path_color` replace text glyphs
/// and panel-draw primitives respectively. `fill_color` and `border_color`
/// recolor only the slot's SDF fill or border role and take precedence over
/// `color` for that role. `border_widths` replaces the slot's authored SDF
/// border widths, which grow inward from the authored outer bounds and so
/// change no geometry layout solved. `offset` translates
/// the slot's SDF, image, text, and panel-line records in the panel-local
/// render frame — panel world units with Y increasing upward — while
/// preserving authored draw depth. `material`
/// replaces the SDF, text, or panel-line source material and re-keys the
/// record when the replacement changes pipeline or resource compatibility.
/// `texture` replaces an image record's sampled texture and re-keys it to
/// the destination `ImageBatchKey`.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct VisualSlotOverride {
    /// Fallback replacement color for authored SDF fill, SDF border, and image tint.
    pub color:         Option<Color>,
    /// Replacement color for text glyphs only.
    pub text_color:    Option<Color>,
    /// Replacement color for panel draw primitives only.
    pub path_color:    Option<Color>,
    /// Replacement color for the SDF fill role only.
    pub fill_color:    Option<Color>,
    /// Replacement color for the SDF border role only.
    pub border_color:  Option<Color>,
    /// Replacement per-side SDF border widths [top, right, bottom, left] in
    /// panel-local world units, the same frame and order
    /// `ResolvedSdfSurface::border_widths` carries.
    pub border_widths: Option<[f32; 4]>,
    /// Panel-local render-frame XY translation added to retained record
    /// transforms: panel world units with Y increasing upward, distinct from
    /// the layout-point frame (Y increasing downward) the widget slot boxes
    /// use. Produce it from a layout-frame delta with
    /// [`layout_delta_to_render_offset`].
    pub offset:        Option<Vec2>,
    /// Replacement source material for SDF, text, and panel-line records.
    pub material:      Option<Handle<StandardMaterial>>,
    /// Replacement sampled texture for image records.
    pub texture:       Option<Handle<Image>>,
}

const _: () = assert!(size_of::<VisualSlotOverride>() <= 184);

impl VisualSlotOverride {
    fn apply(&mut self, overlay: &Self) {
        self.color = overlay.color.or(self.color);
        self.text_color = overlay.text_color.or(self.text_color);
        self.path_color = overlay.path_color.or(self.path_color);
        self.fill_color = overlay.fill_color.or(self.fill_color);
        self.border_color = overlay.border_color.or(self.border_color);
        self.border_widths = overlay.border_widths.or(self.border_widths);
        self.offset = overlay.offset.or(self.offset);
        if overlay.material.is_some() {
            self.material.clone_from(&overlay.material);
        }
        if overlay.texture.is_some() {
            self.texture.clone_from(&overlay.texture);
        }
    }

    fn apply_element(&mut self, overlay: &Self) {
        let offset = self.offset;
        self.apply(overlay);
        if offset.is_some() {
            self.offset = offset;
        }
    }
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
    pub(crate) const fn with_text_color(mut self, color: Color) -> Self {
        self.text_color = Some(color);
        self
    }

    #[must_use]
    pub(crate) const fn with_path_color(mut self, color: Color) -> Self {
        self.path_color = Some(color);
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
    pub(crate) const fn with_border_widths(mut self, widths: [f32; 4]) -> Self {
        self.border_widths = Some(widths);
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
    subtree_color: Option<Color>,
    slots:         Vec<(VisualSlotId, VisualSlotOverride)>,
    elements:      Vec<(usize, VisualSlotOverride)>,
}

impl WidgetVisualOverrides {
    /// Recolors every retained record authored inside the widget subtree.
    pub(crate) const fn set_subtree_color(&mut self, color: Option<Color>) {
        self.subtree_color = color;
    }

    #[must_use]
    const fn subtree_color(&self) -> Option<Color> { self.subtree_color }

    /// Returns the stored override for `slot`.
    #[cfg(test)]
    #[must_use]
    pub(crate) fn get(&self, slot: VisualSlotId) -> Option<&VisualSlotOverride> {
        self.slots
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
    /// re-uploads. Production writers go through [`write_widget_overrides`],
    /// which compares immutably before taking a mutable component reference,
    /// keeping unchanged frames out of `Changed<WidgetVisualOverrides>`
    /// entirely.
    pub(crate) fn set(&mut self, slot: VisualSlotId, value: VisualSlotOverride) {
        match self.slots.iter_mut().find(|(id, _)| *id == slot) {
            Some((_, current)) => {
                if *current != value {
                    *current = value;
                }
            },
            None => self.slots.push((slot, value)),
        }
    }

    /// Removes the override for `slot`, restoring authored presentation.
    #[cfg(test)]
    pub(crate) fn clear(&mut self, slot: VisualSlotId) { self.slots.retain(|(id, _)| *id != slot); }

    /// Sets or replaces an element override while preserving element-index order.
    pub(crate) fn set_element(&mut self, element_index: usize, value: VisualSlotOverride) {
        let insertion_index = self
            .elements
            .partition_point(|(existing_index, _)| *existing_index < element_index);
        match self.elements.get_mut(insertion_index) {
            Some((existing_index, current)) if *existing_index == element_index => {
                if *current != value {
                    *current = value;
                }
            },
            Some(_) | None => self
                .elements
                .insert(insertion_index, (element_index, value)),
        }
    }

    fn slot_overrides(&self) -> impl Iterator<Item = (VisualSlotId, &VisualSlotOverride)> {
        self.slots.iter().map(|(slot, value)| (*slot, value))
    }

    fn element_overrides(&self) -> &[(usize, VisualSlotOverride)] { &self.elements }
}

/// Collects the widgets of `kind` whose presentation inputs moved this frame.
///
/// `changed` carries the entities a per-widget `Changed` filter matched and
/// `removed` the entities drained from that widget's [`RemovedComponents`]
/// streams; a removal names an entity with no live component to read, so its
/// kind comes from `kinds` instead. Both are consumed here, so a quiet frame
/// never walks the live widgets.
pub(crate) fn dirty_widgets(
    changed: impl Iterator<Item = (Entity, WidgetKind)>,
    removed: impl Iterator<Item = Entity>,
    kinds: &Query<&WidgetKind, With<WidgetOf>>,
    kind: WidgetKind,
) -> HashSet<Entity> {
    let mut dirty: HashSet<Entity> = changed
        .filter_map(|(entity, changed_kind)| (changed_kind == kind).then_some(entity))
        .collect();
    dirty.extend(removed.filter(|&entity| kinds.get(entity) == Ok(&kind)));
    dirty
}

/// Resolves sparse part appearances onto their retained-record recipients.
///
/// `WidgetVisualSlots::elements` and `WidgetVisualSlots::part_appearances`
/// are both ordered by element index. Advancing the part cursor only as each
/// recipient passes it visits the two lists in linear time and avoids a
/// per-recipient lookup.
pub(crate) fn resolve_part_overrides(
    desired: &mut WidgetVisualOverrides,
    slots: &WidgetVisualSlots,
    active: &[Option<super::WidgetState>],
    panel: Option<&DiegeticPanel>,
) {
    let part_appearances = slots.part_appearances();
    let mut part_cursor = 0;
    for &(element_index, _) in slots.elements() {
        while part_appearances
            .get(part_cursor)
            .is_some_and(|(part_index, _)| *part_index < element_index)
        {
            part_cursor += 1;
        }
        let Some((part_index, appearance)) = part_appearances.get(part_cursor) else {
            continue;
        };
        if *part_index != element_index {
            continue;
        }
        part_cursor += 1;
        let resolved = appearance.cascades().resolve(active, panel);
        if resolved != VisualSlotOverride::default() {
            desired.set_element(element_index, resolved);
        }
    }
}

/// Replaces one widget's complete desired presentation override, touching
/// mutable state only when the component value changes.
pub(crate) fn write_widget_overrides(
    widget: Entity,
    desired: WidgetVisualOverrides,
    overrides: &mut Query<&mut WidgetVisualOverrides>,
    commands: &mut Commands<'_, '_>,
) {
    let Ok(current) = overrides.get(widget) else {
        if desired != WidgetVisualOverrides::default() {
            commands.entity(widget).insert(desired);
        }
        return;
    };
    if *current == desired {
        return;
    }
    if desired == WidgetVisualOverrides::default() {
        commands.entity(widget).remove::<WidgetVisualOverrides>();
        return;
    }
    let Ok(mut current) = overrides.get_mut(widget) else {
        return;
    };
    *current = desired;
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
            let mut by_element = HashMap::<usize, VisualSlotOverride>::new();
            if let Some(color) = overrides.subtree_color() {
                for &(element_index, _) in slots.elements() {
                    by_element.insert(
                        element_index,
                        VisualSlotOverride {
                            color: Some(color),
                            text_color: Some(color),
                            path_color: Some(color),
                            ..VisualSlotOverride::default()
                        },
                    );
                }
            }
            for (slot, value) in overrides.slot_overrides() {
                let Some(element_index) = slots.element_index(slot) else {
                    continue;
                };
                by_element
                    .entry(element_index)
                    .and_modify(|existing| existing.apply(value))
                    .or_insert_with(|| value.clone());
            }
            for &(element_index, ref value) in overrides.element_overrides() {
                by_element
                    .entry(element_index)
                    .and_modify(|existing| existing.apply_element(value))
                    .or_insert_with(|| value.clone());
            }
            by_element
                .into_iter()
                .map(|(element_index, value)| ((widget_of.panel(), element_index), value))
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
    use bevy::ecs::system::RunSystemOnce;
    use bevy::picking::hover::PickingInteraction;
    use bevy::prelude::*;

    use super::ComputedVisualSlot;
    use super::StateAppearance;
    use super::VisualElementCapabilities;
    use super::VisualOverrideIndex;
    use super::VisualSlotId;
    use super::VisualSlotOverride;
    use super::WidgetVisualOverrides;
    use super::WidgetVisualSlots;
    use crate::Appearance;
    use crate::DiegeticPanel;
    use crate::DiegeticPanelCommands;
    use crate::El;
    use crate::HeadlessLayoutPlugin;
    use crate::ImeAppOwnedFieldSpec;
    use crate::ImeEditableFieldSpec;
    use crate::LayoutBuilder;
    use crate::Mm;
    use crate::PanelElementId;
    use crate::PanelWidgetReader;
    use crate::WidgetHoveredAppearance;
    use crate::cascade::Cascade;
    use crate::cascade::CascadeDefault;
    use crate::layout::BoundingBox;
    use crate::layout::PanelCircle;
    use crate::layout::PanelDraw;
    use crate::layout::Text;
    use crate::layout::TextStyle;
    use crate::text::DiegeticTextMeasurer;
    use crate::widgets::PanelWidget;
    use crate::widgets::Slider;
    use crate::widgets::WidgetOf;
    use crate::widgets::WidgetsPlugin;

    const SLOT: VisualSlotId = VisualSlotId::new(7);
    const SLOT_ELEMENT_INDEX: usize = 3;
    const OVERRIDE_COLOR: Color = Color::srgb(0.9, 0.1, 0.2);
    const PEER_ELEMENT_INDEX: usize = 5;
    const PEER_OVERRIDE_COLOR: Color = Color::srgb(0.2, 0.8, 0.9);
    const PART_HOVER_FILL: Color = Color::srgb(0.3, 0.7, 0.2);
    const TEXT_OVERRIDE_COLOR: Color = Color::srgb(0.3, 0.2, 0.7);
    const PATH_OVERRIDE_COLOR: Color = Color::srgb(0.7, 0.2, 0.3);

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

    fn widgets_test_app() -> App {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .insert_resource(DiegeticTextMeasurer::default())
            .add_plugins((HeadlessLayoutPlugin, WidgetsPlugin));
        app
    }

    fn spawn_widget_panel(app: &mut App, tree: crate::LayoutTree) -> Entity {
        let panel = DiegeticPanel::world()
            .size(Mm(100.0), Mm(50.0))
            .with_tree(tree)
            .build()
            .expect("widget panel should build");
        app.world_mut().spawn(panel).id()
    }

    fn resolve_widget(app: &mut App, panel: Entity, id: &'static str) -> Entity {
        app.world_mut()
            .run_system_once(move |reader: PanelWidgetReader| {
                reader.entity(panel, &PanelElementId::named(id))
            })
            .ok()
            .flatten()
            .expect("widget should reify")
    }

    fn reauthor_tree(app: &mut App, panel: Entity, tree: crate::LayoutTree) {
        app.world_mut()
            .commands()
            .set_tree(panel, tree)
            .expect("replacement tree should be accepted");
        app.update();
        app.update();
    }

    fn indexed_fill(app: &App, panel: Entity, element_index: usize) -> Option<Color> {
        app.world()
            .resource::<VisualOverrideIndex>()
            .get(panel, element_index)
            .and_then(|override_value| override_value.fill_color)
    }

    fn assert_no_unowned_slider_part_overrides(
        app: &App,
        panel: Entity,
        widget: Entity,
        styled_unauthored_index: usize,
        structural_container_index: usize,
    ) {
        let visual_override_index = app.world().resource::<VisualOverrideIndex>();
        assert!(
            visual_override_index
                .get(panel, styled_unauthored_index)
                .is_none(),
            "a visual recipient without an authored state appearance receives no element override",
        );
        assert!(
            visual_override_index
                .get(panel, structural_container_index)
                .is_none(),
            "a pure slider layout container creates no visual override index entry",
        );
        assert!(
            !app.world()
                .get::<WidgetVisualSlots>(widget)
                .expect("slider widget visual slots")
                .elements()
                .iter()
                .any(|(element_index, _)| *element_index == structural_container_index),
            "a pure slider layout container is not a retained-record recipient",
        );
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

    fn hovered_part_appearance() -> StateAppearance {
        StateAppearance {
            hovered: Cascade::Override(WidgetHoveredAppearance::new(
                Appearance::new().background(PART_HOVER_FILL),
            )),
            ..StateAppearance::default()
        }
    }

    fn text_part_tree(appearance: Appearance) -> (crate::LayoutTree, usize) {
        let mut builder = LayoutBuilder::new(100.0, 50.0);
        builder.with(El::new().button("button"), |children| {
            children.text(
                Text::new("label", TextStyle::new(10.0)).layout(El::new().hovered(appearance)),
            );
        });
        let tree = builder.build();
        (tree.clone(), tree.len() - 1)
    }

    fn text_and_draw_part_tree(appearance: Appearance) -> (crate::LayoutTree, usize) {
        let mut builder = LayoutBuilder::new(100.0, 50.0);
        builder.with(El::new().button("button"), |children| {
            children.text(
                Text::new("label", TextStyle::new(10.0)).layout(
                    El::new()
                        .draw(PanelDraw::shapes([PanelCircle::new((10.0, 10.0), 5.0)]))
                        .hovered(appearance),
                ),
            );
        });
        let tree = builder.build();
        (tree.clone(), tree.len() - 1)
    }

    fn button_part_tree(stateful_part: bool, prepended_part: bool) -> (crate::LayoutTree, usize) {
        let mut builder = LayoutBuilder::new(100.0, 50.0);
        builder.with(
            El::new().background(Color::WHITE).button("button"),
            |children| {
                if prepended_part {
                    children.with(El::new().background(Color::WHITE), |_| {});
                }
                children.with(El::new().background(Color::WHITE), |_| {});
            },
        );
        let mut tree = builder.build();
        let part_index = tree.len() - 1;
        if stateful_part {
            assert!(tree.set_element_state_appearance(part_index, hovered_part_appearance()));
        }
        (tree, part_index)
    }

    fn editable_part_tree(stateful_part: bool, prepended_part: bool) -> (crate::LayoutTree, usize) {
        let mut builder = LayoutBuilder::new(100.0, 50.0);
        let field = ImeEditableFieldSpec::AppOwned(ImeAppOwnedFieldSpec::new("test"));
        builder.with(
            El::new()
                .background(Color::WHITE)
                .editable_field("editable", field),
            |children| {
                if prepended_part {
                    children.with(El::new().background(Color::WHITE), |_| {});
                }
                children.with(El::new().background(Color::WHITE), |_| {});
            },
        );
        let mut tree = builder.build();
        let part_index = tree.len() - 1;
        if stateful_part {
            assert!(tree.set_element_state_appearance(part_index, hovered_part_appearance()));
        }
        (tree, part_index)
    }

    fn slider_part_tree(
        stateful_part: bool,
        prepended_part: bool,
    ) -> (crate::LayoutTree, usize, usize, usize) {
        let mut builder = LayoutBuilder::new(100.0, 50.0);
        builder.with(
            El::overlay()
                .size(40.0, 16.0)
                .background(Color::WHITE)
                .widget("slider", Slider::new(0.0..=1.0)),
            |children| {
                if prepended_part {
                    children.with(El::new().background(Color::WHITE), |_| {});
                }
                children.with(El::new().background(Color::WHITE), |_| {});
                children.with(El::new().background(Color::WHITE), |_| {});
                children.with(El::new(), |_| {});
            },
        );
        let mut tree = builder.build();
        let structural_container_index = tree.len() - 1;
        let styled_unauthored_index = structural_container_index - 1;
        let part_index = styled_unauthored_index - 1;
        if stateful_part {
            assert!(tree.set_element_state_appearance(part_index, hovered_part_appearance()));
        }
        (
            tree,
            part_index,
            styled_unauthored_index,
            structural_container_index,
        )
    }

    #[test]
    fn element_override_composes_over_a_slot_without_replacing_its_offset() {
        let mut app = dispatch_app();
        let panel = app.world_mut().spawn_empty().id();
        let slots =
            WidgetVisualSlots::new(vec![computed_slot(SLOT, SLOT_ELEMENT_INDEX)]).with_elements(
                vec![(SLOT_ELEMENT_INDEX, VisualElementCapabilities::default())],
            );
        let mut overrides = WidgetVisualOverrides::default();
        let offset = Vec2::new(2.0, -3.0);
        overrides.set(SLOT, VisualSlotOverride::default().with_offset(offset));
        overrides.set_element(
            SLOT_ELEMENT_INDEX,
            VisualSlotOverride::default().with_fill_color(OVERRIDE_COLOR),
        );
        app.world_mut().spawn((
            PanelWidget::new(PanelElementId::named("styled")),
            WidgetOf::new(panel),
            slots,
            overrides,
        ));

        app.update();

        assert_eq!(
            app.world()
                .resource::<VisualOverrideIndex>()
                .get(panel, SLOT_ELEMENT_INDEX),
            Some(&VisualSlotOverride {
                fill_color: Some(OVERRIDE_COLOR),
                offset: Some(offset),
                ..VisualSlotOverride::default()
            }),
        );
    }

    #[test]
    fn element_text_and_path_colors_compose_over_a_slot_override() {
        let mut app = dispatch_app();
        let panel = app.world_mut().spawn_empty().id();
        let slots = WidgetVisualSlots::new(vec![computed_slot(SLOT, SLOT_ELEMENT_INDEX)])
            .with_elements(vec![(SLOT_ELEMENT_INDEX, VisualElementCapabilities::TEXT)]);
        let mut overrides = WidgetVisualOverrides::default();
        overrides.set(
            SLOT,
            VisualSlotOverride::default().with_color(OVERRIDE_COLOR),
        );
        overrides.set_element(
            SLOT_ELEMENT_INDEX,
            VisualSlotOverride::default()
                .with_text_color(TEXT_OVERRIDE_COLOR)
                .with_path_color(PATH_OVERRIDE_COLOR),
        );
        app.world_mut().spawn((
            PanelWidget::new(PanelElementId::named("styled")),
            WidgetOf::new(panel),
            slots,
            overrides,
        ));

        app.update();

        assert_eq!(
            app.world()
                .resource::<VisualOverrideIndex>()
                .get(panel, SLOT_ELEMENT_INDEX),
            Some(&VisualSlotOverride {
                color: Some(OVERRIDE_COLOR),
                text_color: Some(TEXT_OVERRIDE_COLOR),
                path_color: Some(PATH_OVERRIDE_COLOR),
                ..VisualSlotOverride::default()
            }),
        );
    }

    #[test]
    fn text_only_part_state_color_never_creates_a_fill_capability_or_override() {
        let mut app = widgets_test_app();
        let (tree, text_index) = text_part_tree(Appearance::new().text_color(TEXT_OVERRIDE_COLOR));
        let panel = spawn_widget_panel(&mut app, tree);
        app.update();
        let widget = resolve_widget(&mut app, panel, "button");
        let capabilities = app
            .world()
            .get::<WidgetVisualSlots>(widget)
            .expect("button should carry visual slots")
            .elements()
            .iter()
            .find(|(element_index, _)| *element_index == text_index)
            .map(|(_, capabilities)| *capabilities)
            .expect("text recipient should remain in visual slots");
        assert!(capabilities.contains(VisualElementCapabilities::TEXT));
        assert!(!capabilities.contains(VisualElementCapabilities::SDF_FILL));

        app.world_mut()
            .entity_mut(widget)
            .insert(PickingInteraction::Hovered);
        app.update();

        let override_value = app
            .world()
            .resource::<VisualOverrideIndex>()
            .get(panel, text_index)
            .expect("hovered text recipient should receive an override");
        assert_eq!(override_value.text_color, Some(TEXT_OVERRIDE_COLOR));
        assert_eq!(override_value.fill_color, None);
    }

    #[test]
    fn global_text_color_default_stays_dormant_on_textless_widget_part() {
        let mut app = widgets_test_app();
        app.insert_resource(CascadeDefault(WidgetHoveredAppearance::new(
            Appearance::new().text_color(TEXT_OVERRIDE_COLOR),
        )));
        let mut builder = LayoutBuilder::new(100.0, 50.0);
        builder.with(El::new().button("button"), |children| {
            children.with(El::new().id("container"), |_| {});
        });
        let tree = builder.build();
        let container_index = tree.len() - 1;
        let panel = DiegeticPanel::world()
            .size(Mm(100.0), Mm(50.0))
            .with_tree(tree)
            .build()
            .expect("global text-color default should not reject a textless widget part");
        let panel = app.world_mut().spawn(panel).id();
        app.update();
        let widget = resolve_widget(&mut app, panel, "button");

        app.world_mut()
            .entity_mut(widget)
            .insert(PickingInteraction::Hovered);
        app.update();

        assert_eq!(
            app.world()
                .resource::<VisualOverrideIndex>()
                .get(panel, container_index)
                .and_then(|override_value| override_value.text_color),
            None,
        );
    }

    #[test]
    fn text_and_draw_part_state_colors_stay_role_isolated() {
        let mut app = widgets_test_app();
        let (tree, text_index) =
            text_and_draw_part_tree(Appearance::new().text_color(TEXT_OVERRIDE_COLOR));
        let panel = spawn_widget_panel(&mut app, tree);
        app.update();
        let widget = resolve_widget(&mut app, panel, "button");
        let capabilities = app
            .world()
            .get::<WidgetVisualSlots>(widget)
            .expect("button should carry visual slots")
            .elements()
            .iter()
            .find(|(element_index, _)| *element_index == text_index)
            .map(|(_, capabilities)| *capabilities)
            .expect("text and draw recipient should remain in visual slots");
        assert!(capabilities.contains(VisualElementCapabilities::TEXT));
        assert!(capabilities.contains(VisualElementCapabilities::DRAW));

        app.world_mut()
            .entity_mut(widget)
            .insert(PickingInteraction::Hovered);
        app.update();
        let text_override = app
            .world()
            .resource::<VisualOverrideIndex>()
            .get(panel, text_index)
            .expect("hovered text and draw recipient should receive an override");
        assert_eq!(text_override.text_color, Some(TEXT_OVERRIDE_COLOR));
        assert_eq!(text_override.path_color, None);

        let (tree, path_index) =
            text_and_draw_part_tree(Appearance::new().path_color(PATH_OVERRIDE_COLOR));
        assert_eq!(path_index, text_index);
        reauthor_tree(&mut app, panel, tree);
        let path_override = app
            .world()
            .resource::<VisualOverrideIndex>()
            .get(panel, path_index)
            .expect("re-authored text and draw recipient should receive an override");
        assert_eq!(path_override.text_color, None);
        assert_eq!(path_override.path_color, Some(PATH_OVERRIDE_COLOR));
    }

    #[test]
    fn non_root_part_appearance_reaches_its_owned_element() {
        let mut app = widgets_test_app();
        let (tree, part_index) = button_part_tree(true, false);
        let panel = spawn_widget_panel(&mut app, tree);
        app.update();
        let widget = resolve_widget(&mut app, panel, "button");

        app.world_mut()
            .entity_mut(widget)
            .insert(PickingInteraction::Hovered);
        app.update();

        assert_eq!(indexed_fill(&app, panel, part_index), Some(PART_HOVER_FILL));
    }

    #[test]
    fn button_part_reauthoring_clears_and_moves_element_overrides() {
        let mut app = widgets_test_app();
        let (tree, original_part_index) = button_part_tree(true, false);
        let panel = spawn_widget_panel(&mut app, tree);
        app.update();
        let widget = resolve_widget(&mut app, panel, "button");
        app.world_mut()
            .entity_mut(widget)
            .insert(PickingInteraction::Hovered);
        app.update();
        assert_eq!(
            indexed_fill(&app, panel, original_part_index),
            Some(PART_HOVER_FILL)
        );

        let (tree, moved_part_index) = button_part_tree(true, true);
        assert_ne!(moved_part_index, original_part_index);
        reauthor_tree(&mut app, panel, tree);
        assert_eq!(resolve_widget(&mut app, panel, "button"), widget);
        assert_eq!(
            indexed_fill(&app, panel, moved_part_index),
            Some(PART_HOVER_FILL)
        );
        assert_eq!(indexed_fill(&app, panel, original_part_index), None);

        let (tree, restored_part_index) = button_part_tree(true, false);
        assert_eq!(restored_part_index, original_part_index);
        reauthor_tree(&mut app, panel, tree);
        assert_eq!(
            indexed_fill(&app, panel, original_part_index),
            Some(PART_HOVER_FILL)
        );

        let (tree, cleared_part_index) = button_part_tree(false, false);
        assert_eq!(cleared_part_index, original_part_index);
        reauthor_tree(&mut app, panel, tree);
        assert_eq!(indexed_fill(&app, panel, original_part_index), None);
    }

    #[test]
    fn editable_part_reauthoring_clears_and_moves_element_overrides() {
        let mut app = widgets_test_app();
        let (tree, original_part_index) = editable_part_tree(true, false);
        let panel = spawn_widget_panel(&mut app, tree);
        app.update();
        let widget = resolve_widget(&mut app, panel, "editable");
        app.world_mut()
            .entity_mut(widget)
            .insert(PickingInteraction::Hovered);
        app.update();
        assert_eq!(
            indexed_fill(&app, panel, original_part_index),
            Some(PART_HOVER_FILL)
        );

        let (tree, moved_part_index) = editable_part_tree(true, true);
        assert_ne!(moved_part_index, original_part_index);
        reauthor_tree(&mut app, panel, tree);
        assert_eq!(resolve_widget(&mut app, panel, "editable"), widget);
        assert_eq!(
            indexed_fill(&app, panel, moved_part_index),
            Some(PART_HOVER_FILL)
        );
        assert_eq!(indexed_fill(&app, panel, original_part_index), None);

        let (tree, restored_part_index) = editable_part_tree(true, false);
        assert_eq!(restored_part_index, original_part_index);
        reauthor_tree(&mut app, panel, tree);
        assert_eq!(
            indexed_fill(&app, panel, original_part_index),
            Some(PART_HOVER_FILL)
        );

        let (tree, cleared_part_index) = editable_part_tree(false, false);
        assert_eq!(cleared_part_index, original_part_index);
        reauthor_tree(&mut app, panel, tree);
        assert_eq!(indexed_fill(&app, panel, original_part_index), None);
    }

    #[test]
    fn slider_part_reauthoring_clears_and_moves_only_part_element_overrides() {
        let mut app = widgets_test_app();
        let (tree, original_part_index, styled_unauthored_index, structural_container_index) =
            slider_part_tree(true, false);
        let panel = spawn_widget_panel(&mut app, tree);
        app.update();
        let widget = resolve_widget(&mut app, panel, "slider");
        app.world_mut()
            .entity_mut(widget)
            .insert(PickingInteraction::Hovered);
        app.update();
        assert_eq!(
            indexed_fill(&app, panel, original_part_index),
            Some(PART_HOVER_FILL)
        );
        assert_no_unowned_slider_part_overrides(
            &app,
            panel,
            widget,
            styled_unauthored_index,
            structural_container_index,
        );

        let (
            tree,
            moved_part_index,
            moved_styled_unauthored_index,
            moved_structural_container_index,
        ) = slider_part_tree(true, true);
        assert_ne!(moved_part_index, original_part_index);
        reauthor_tree(&mut app, panel, tree);
        assert_eq!(
            indexed_fill(&app, panel, moved_part_index),
            Some(PART_HOVER_FILL)
        );
        assert_eq!(indexed_fill(&app, panel, original_part_index), None);
        assert_no_unowned_slider_part_overrides(
            &app,
            panel,
            widget,
            moved_styled_unauthored_index,
            moved_structural_container_index,
        );

        let (
            tree,
            restored_part_index,
            restored_styled_unauthored_index,
            restored_structural_container_index,
        ) = slider_part_tree(true, false);
        assert_eq!(restored_part_index, original_part_index);
        reauthor_tree(&mut app, panel, tree);
        assert_eq!(
            indexed_fill(&app, panel, original_part_index),
            Some(PART_HOVER_FILL)
        );
        assert_no_unowned_slider_part_overrides(
            &app,
            panel,
            widget,
            restored_styled_unauthored_index,
            restored_structural_container_index,
        );

        let (
            tree,
            cleared_part_index,
            cleared_styled_unauthored_index,
            cleared_structural_container_index,
        ) = slider_part_tree(false, false);
        assert_eq!(cleared_part_index, original_part_index);
        reauthor_tree(&mut app, panel, tree);
        assert_eq!(resolve_widget(&mut app, panel, "slider"), widget);
        assert_eq!(indexed_fill(&app, panel, original_part_index), None);
        assert_no_unowned_slider_part_overrides(
            &app,
            panel,
            widget,
            cleared_styled_unauthored_index,
            cleared_structural_container_index,
        );
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
