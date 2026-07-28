//! Per-state appearance for widget elements.
//!
//! An element's resting appearance stays on its ordinary declarations —
//! [`El::background`](crate::El::background), [`El::border`](crate::El::border),
//! and [`El::material`](crate::El::material). A widget element adds a
//! [`StateAppearance`]: one [`Appearance`] per [`WidgetState`], naming only the
//! properties that state replaces on the widget's root visual slot.
//! [`StateAppearance::resolve`] layers the active states in
//! [`WidgetState::LAYER_ORDER`] and returns the [`VisualSlotOverride`] the
//! retained routes apply, so state presentation patches records layout already
//! emitted and never re-authors layout.

use core::mem::size_of;
use std::sync::Arc;

use bevy::prelude::*;

use super::VisualSlotOverride;
use crate::DiegeticPanel;
use crate::cascade::CASCADE_ATTRIBUTE_BYTES;
use crate::cascade::Cascade;
use crate::layout::Dimension;

/// One widget state's decision for a single visual property.
#[derive(Clone, Debug, Default, PartialEq, Reflect)]
pub(crate) enum VisualChange<T> {
    /// The state keeps whatever value the prior layer resolved.
    #[default]
    Unchanged,
    /// The state replaces the property with this value.
    To(T),
}

impl<T> VisualChange<T> {
    /// Whether this layer authors a replacement.
    pub(crate) const fn is_authored(&self) -> bool { matches!(self, Self::To(_)) }

    /// Consumes the layer and returns its replacement value.
    fn into_value(self) -> Option<T> {
        match self {
            Self::Unchanged => None,
            Self::To(value) => Some(value),
        }
    }
}

impl<T: Clone> VisualChange<T> {
    /// Overwrites `resolved` when this layer authors a replacement.
    fn layer_onto(&self, resolved: &mut Self) {
        if let Self::To(value) = self {
            *resolved = Self::To(value.clone());
        }
    }
}

/// The visual properties a widget state replaces.
///
/// State methods on [`crate::El`] accept one `Appearance` bundle. Each builder
/// below names a retained record that an ordinary element declaration must
/// already have emitted:
///
/// | Builder | Retained record | Ordinary declaration required |
/// | --- | --- | --- |
/// | [`Appearance::background`] | Root SDF fill | [`crate::El::background`] |
/// | [`Appearance::border_color`] | Root SDF border color | [`crate::El::border`] |
/// | [`Appearance::border_width`] | Root SDF border widths | [`crate::El::border`] |
/// | [`Appearance::material`] | Root SDF fill or border material | [`crate::El::background`] or [`crate::El::border`] |
///
/// A button and slider can author hovered, focused, pressed, and disabled
/// bundles. An editable field can author hovered, focused, and disabled
/// bundles, but has no pressed state. Each state inherits independently from
/// the lower-precedence cascade scopes before active states layer in this
/// order: focused, hovered, pressed, then disabled. A property named by a
/// later state replaces the same property from an earlier state; a
/// property the bundle does not name keeps the earlier state's result or the
/// ordinary declaration.
///
/// A state bundle only patches an existing retained record. Use
/// `Border::all(Px(0.0), color)` when a state needs to widen a border that is
/// normally invisible, or [`crate::El::background`] with [`Color::NONE`] when
/// a state needs a transparent fill record.
///
/// # Examples
///
/// ```no_run
/// use bevy::color::Color;
/// use hana_diegetic::Appearance;
/// use hana_diegetic::Border;
/// use hana_diegetic::El;
/// use hana_diegetic::Px;
/// use hana_diegetic::Slider;
///
/// let button = El::new()
///     .background(Color::NONE)
///     .border(Border::all(Px(0.0), Color::WHITE))
///     .button("apply")
///     .hovered(Appearance::new().background(Color::BLACK))
///     .focused(Appearance::new().border_width(Px(2.0)))
///     .pressed(Appearance::new().border_color(Color::WHITE))
///     .disabled(Appearance::new().material(Default::default()));
/// let slider = El::new()
///     .background(Color::NONE)
///     .border(Border::all(Px(0.0), Color::WHITE))
///     .widget("level", Slider::new(0.0..=1.0))
///     .hovered(Appearance::new().background(Color::BLACK))
///     .focused(Appearance::new().border_width(Px(2.0)))
///     .pressed(Appearance::new().border_color(Color::WHITE))
///     .disabled(Appearance::new().material(Default::default()));
/// let _ = (button, slider);
/// ```
#[must_use]
#[derive(Clone, Debug, Default, PartialEq, Reflect)]
pub struct Appearance {
    /// Replaces the element's authored background color.
    pub(crate) background:   VisualChange<Color>,
    /// Replaces the element's authored border color.
    pub(crate) border_color: VisualChange<Color>,
    /// Replaces the element's authored border width on all four sides.
    pub(crate) border_width: VisualChange<Dimension>,
    /// Replaces the element's authored root material.
    pub(crate) material:     VisualChange<Handle<StandardMaterial>>,
}

impl Appearance {
    /// Creates a bundle that leaves every property unchanged.
    pub const fn new() -> Self {
        Self {
            background:   VisualChange::Unchanged,
            border_color: VisualChange::Unchanged,
            border_width: VisualChange::Unchanged,
            material:     VisualChange::Unchanged,
        }
    }

    /// Replaces the root SDF fill color.
    ///
    /// The widget element must also call [`crate::El::background`] so layout
    /// emits the retained fill record this value patches.
    pub const fn background(mut self, color: Color) -> Self {
        self.background = VisualChange::To(color);
        self
    }

    /// Replaces the root SDF border color without changing its radii.
    ///
    /// The widget element must also call [`crate::El::border`] so layout emits
    /// the retained border record this value patches.
    pub const fn border_color(mut self, color: Color) -> Self {
        self.border_color = VisualChange::To(color);
        self
    }

    /// Replaces all four root SDF border widths without changing solved layout.
    ///
    /// The widget element must also call [`crate::El::border`]. The retained
    /// border grows inward, leaving the element's outer bounds unchanged.
    pub fn border_width(mut self, width: impl Into<Dimension>) -> Self {
        self.border_width = VisualChange::To(width.into());
        self
    }

    /// Replaces the material for the root SDF fill and border records.
    ///
    /// The widget element must also call [`crate::El::background`] or
    /// [`crate::El::border`] so layout emits a retained surface record.
    pub fn material(mut self, material: Handle<StandardMaterial>) -> Self {
        self.material = VisualChange::To(material);
        self
    }

    /// Applies every property this layer authors over `resolved`.
    fn layer_onto(&self, resolved: &mut Self) {
        self.background.layer_onto(&mut resolved.background);
        self.border_color.layer_onto(&mut resolved.border_color);
        self.border_width.layer_onto(&mut resolved.border_width);
        self.material.layer_onto(&mut resolved.material);
    }

    /// Converts a fully layered appearance into its retained-slot override.
    fn into_slot_override(self, panel: Option<&DiegeticPanel>) -> VisualSlotOverride {
        let border_widths = self
            .border_width
            .into_value()
            .zip(panel)
            .and_then(|(width, panel)| render_border_widths(width, panel));
        VisualSlotOverride {
            fill_color: self.background.into_value(),
            border_color: self.border_color.into_value(),
            border_widths,
            material: self.material.into_value(),
            ..VisualSlotOverride::default()
        }
    }
}

/// Hovered-state cascade attribute for one [`Appearance`] bundle.
///
/// [`crate::El::hovered`] creates this opaque attribute from its bundle.
#[derive(Clone, Debug, Reflect)]
pub struct WidgetHoveredAppearance(Arc<Appearance>);

impl WidgetHoveredAppearance {
    pub(crate) fn new(appearance: Appearance) -> Self { Self(Arc::new(appearance)) }

    fn appearance(&self) -> &Appearance { &self.0 }
}

impl PartialEq for WidgetHoveredAppearance {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.0, &other.0) || self.0.as_ref() == other.0.as_ref()
    }
}

const _: () = assert!(size_of::<WidgetHoveredAppearance>() <= CASCADE_ATTRIBUTE_BYTES);

/// Pressed-state cascade attribute for one [`Appearance`] bundle.
///
/// [`crate::El::pressed`] creates this opaque attribute from its bundle.
#[derive(Clone, Debug, Reflect)]
pub struct WidgetPressedAppearance(Arc<Appearance>);

impl WidgetPressedAppearance {
    pub(crate) fn new(appearance: Appearance) -> Self { Self(Arc::new(appearance)) }

    fn appearance(&self) -> &Appearance { &self.0 }
}

impl PartialEq for WidgetPressedAppearance {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.0, &other.0) || self.0.as_ref() == other.0.as_ref()
    }
}

const _: () = assert!(size_of::<WidgetPressedAppearance>() <= CASCADE_ATTRIBUTE_BYTES);

/// Focused-state cascade attribute for one [`Appearance`] bundle.
///
/// [`crate::El::focused`] creates this opaque attribute from its bundle.
#[derive(Clone, Debug, Reflect)]
pub struct WidgetFocusedAppearance(Arc<Appearance>);

impl WidgetFocusedAppearance {
    pub(crate) fn new(appearance: Appearance) -> Self { Self(Arc::new(appearance)) }

    fn appearance(&self) -> &Appearance { &self.0 }
}

impl PartialEq for WidgetFocusedAppearance {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.0, &other.0) || self.0.as_ref() == other.0.as_ref()
    }
}

const _: () = assert!(size_of::<WidgetFocusedAppearance>() <= CASCADE_ATTRIBUTE_BYTES);

/// Disabled-state cascade attribute for one [`Appearance`] bundle.
///
/// [`crate::El::disabled`] creates this opaque attribute from its bundle.
#[derive(Clone, Debug, Reflect)]
pub struct WidgetDisabledAppearance(Arc<Appearance>);

impl WidgetDisabledAppearance {
    pub(crate) fn new(appearance: Appearance) -> Self { Self(Arc::new(appearance)) }

    fn appearance(&self) -> &Appearance { &self.0 }
}

impl PartialEq for WidgetDisabledAppearance {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.0, &other.0) || self.0.as_ref() == other.0.as_ref()
    }
}

const _: () = assert!(size_of::<WidgetDisabledAppearance>() <= CASCADE_ATTRIBUTE_BYTES);

/// One [`Appearance`] per [`WidgetState`], authored by the state builders on a
/// widget element and carried to the widget entity for state presentation.
#[derive(Clone, Component, Debug, Default, PartialEq)]
pub(crate) struct StateAppearance {
    pub(crate) hovered:  Cascade<WidgetHoveredAppearance>,
    pub(crate) pressed:  Cascade<WidgetPressedAppearance>,
    pub(crate) focused:  Cascade<WidgetFocusedAppearance>,
    pub(crate) disabled: Cascade<WidgetDisabledAppearance>,
}

impl StateAppearance {
    fn layer(&self, state: WidgetState) -> Option<&Appearance> {
        match state {
            WidgetState::Focused => self
                .focused
                .as_override()
                .map(WidgetFocusedAppearance::appearance),
            WidgetState::Hovered => self
                .hovered
                .as_override()
                .map(WidgetHoveredAppearance::appearance),
            WidgetState::Pressed => self
                .pressed
                .as_override()
                .map(WidgetPressedAppearance::appearance),
            WidgetState::Disabled => self
                .disabled
                .as_override()
                .map(WidgetDisabledAppearance::appearance),
        }
    }

    /// Whether any state layer authors the property `authored` selects.
    pub(crate) fn any(&self, authored: impl Fn(&Appearance) -> bool) -> bool {
        WidgetState::LAYER_ORDER
            .into_iter()
            .any(|state| self.layer(state).is_some_and(&authored))
    }

    /// Composes the root-slot override for the active state set.
    ///
    /// Layers apply in [`WidgetState::LAYER_ORDER`] however `active` orders its
    /// entries: each property resolves independently, a state that leaves a
    /// property [`VisualChange::Unchanged`] keeps the prior layer's value, and a
    /// property no active state replaces stays at the element's authored value.
    /// `panel` supplies the unit conversion a border-width replacement needs;
    /// without it the width stays authored while every other property still
    /// resolves.
    pub(crate) fn resolve(
        &self,
        active: &[Option<WidgetState>],
        panel: Option<&DiegeticPanel>,
    ) -> VisualSlotOverride {
        let mut resolved = Appearance::default();
        for state in WidgetState::LAYER_ORDER {
            if active.contains(&Some(state))
                && let Some(layer) = self.layer(state)
            {
                layer.layer_onto(&mut resolved);
            }
        }
        resolved.into_slot_override(panel)
    }
}

/// One interaction state a widget element authors an [`Appearance`] for.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum WidgetState {
    /// The widget's keyboard focus indicator is visible.
    Focused,
    /// A pointer is over the widget.
    Hovered,
    /// The widget is held: a button press or a slider drag.
    Pressed,
    /// The widget refuses interaction.
    Disabled,
}

impl WidgetState {
    /// Layering order for [`StateAppearance::resolve`]: a later state replaces
    /// the properties it authors over an earlier one.
    pub(crate) const LAYER_ORDER: [Self; 4] =
        [Self::Focused, Self::Hovered, Self::Pressed, Self::Disabled];
}

/// Converts an authored state border width into the panel-local world widths
/// retained SDF records carry.
///
/// Mirrors the authored border path: the dimension resolves to layout points
/// against the panel's layout unit, then scales by the panel's points-to-world
/// factor. Returns `None` for a negative or non-finite width, or a non-finite
/// or non-positive panel scale, so an unresolvable width leaves the authored
/// widths in place instead of writing a manufactured one.
fn render_border_widths(width: Dimension, panel: &DiegeticPanel) -> Option<[f32; 4]> {
    let points = width.to_points(panel.layout_unit().to_points());
    let points_to_world = panel.points_to_world();
    if !points.is_finite() || points < 0.0 || !points_to_world.is_finite() || points_to_world <= 0.0
    {
        return None;
    }
    Some([points * points_to_world; 4])
}

#[cfg(test)]
#[allow(
    clippy::expect_used,
    reason = "tests should panic on unexpected values"
)]
mod tests {
    use bevy::prelude::*;

    use super::Appearance;
    use super::StateAppearance;
    use super::WidgetFocusedAppearance;
    use super::WidgetHoveredAppearance;
    use super::WidgetPressedAppearance;
    use super::WidgetState;
    use crate::DiegeticPanel;
    use crate::Mm;
    use crate::Px;
    use crate::cascade::Cascade;

    const HOVER_FILL: Color = Color::srgb(0.1, 0.2, 0.3);
    const FOCUS_BORDER: Color = Color::srgb(0.9, 0.8, 0.2);
    const PRESS_FILL: Color = Color::srgb(0.4, 0.5, 0.6);

    fn panel() -> DiegeticPanel {
        DiegeticPanel::world()
            .size(Mm(100.0), Mm(50.0))
            .build()
            .expect("a sized world panel builds")
    }

    fn state_appearance() -> StateAppearance {
        StateAppearance {
            hovered: Cascade::Override(WidgetHoveredAppearance::new(
                Appearance::new().background(HOVER_FILL),
            )),
            pressed: Cascade::Override(WidgetPressedAppearance::new(
                Appearance::new().background(PRESS_FILL),
            )),
            focused: Cascade::Override(WidgetFocusedAppearance::new(
                Appearance::new()
                    .border_color(FOCUS_BORDER)
                    .border_width(Px(2.0)),
            )),
            ..StateAppearance::default()
        }
    }

    #[test]
    fn properties_layer_independently() {
        // Focus authors only the border and hover only the fill, so both
        // survive when the two states are live together.
        let resolved = state_appearance().resolve(
            &[Some(WidgetState::Focused), Some(WidgetState::Hovered)],
            None,
        );
        assert_eq!(resolved.fill_color, Some(HOVER_FILL));
        assert_eq!(resolved.border_color, Some(FOCUS_BORDER));
    }

    #[test]
    fn later_layers_replace_earlier_ones_regardless_of_active_order() {
        // Pressed sits after hovered in the layer order, so it wins the fill
        // even when the caller lists hovered last.
        let resolved = state_appearance().resolve(
            &[Some(WidgetState::Pressed), Some(WidgetState::Hovered)],
            None,
        );
        assert_eq!(resolved.fill_color, Some(PRESS_FILL));
    }

    #[test]
    fn inactive_states_author_nothing() {
        let resolved = state_appearance().resolve(&[None, None, None, None], None);
        assert_eq!(resolved.fill_color, None);
        assert_eq!(resolved.border_color, None);
        assert_eq!(resolved.border_widths, None);
    }

    #[test]
    fn border_width_resolves_through_the_owning_panel_scale() {
        let panel = panel();
        let resolved = state_appearance().resolve(&[Some(WidgetState::Focused)], Some(&panel));
        let widths = resolved
            .border_widths
            .expect("a focused width and a live panel resolve to render widths");
        // Two logical pixels resolve to points through the fixed 96 DPI
        // convention, then scale to panel world units.
        let expected = 2.0 * 0.75 * panel.points_to_world();
        for width in widths {
            assert!(
                (width - expected).abs() < 1e-6,
                "each side carries the same resolved width",
            );
        }
    }

    #[test]
    fn border_width_without_a_panel_leaves_the_authored_width() {
        let resolved = state_appearance().resolve(&[Some(WidgetState::Focused)], None);
        assert_eq!(resolved.border_widths, None);
        assert_eq!(
            resolved.border_color,
            Some(FOCUS_BORDER),
            "a missing panel must not suppress the other properties",
        );
    }
}
