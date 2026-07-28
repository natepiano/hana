//! Editable-field widget presentation.

use bevy::picking::hover::PickingInteraction;
use bevy::prelude::*;

use super::VisualSlotId;
#[cfg(test)]
use super::VisualSlotOverride;
use super::WidgetDisabled;
use super::WidgetFocusVisible;
use super::WidgetKind;
use super::WidgetOf;
use super::WidgetState;
use super::WidgetStateCascades;
use super::WidgetVisualOverrides;
use super::WidgetVisualSlots;
use super::visual;
use crate::DiegeticPanel;
use crate::cascade::Cascade;

/// Reports whether an editable field's authored presentation or presented state
/// changed since the last run.
///
/// The changed query filters to [`WidgetKind::EditableField`] so an unrelated
/// button or slider change never wakes the all-field walk. Each
/// [`RemovedComponents`] stream is drained every run and its removals are kept
/// only for entities still live as editable fields, reporting the edges back to
/// normal.
pub(super) fn presentation_inputs_changed(
    changed: Query<
        &WidgetKind,
        (
            With<WidgetOf>,
            Or<(
                Changed<Cascade<super::WidgetHoveredAppearance>>,
                Changed<Cascade<super::WidgetPressedAppearance>>,
                Changed<Cascade<super::WidgetFocusedAppearance>>,
                Changed<Cascade<super::WidgetDisabledAppearance>>,
                Changed<WidgetVisualSlots>,
                Changed<WidgetFocusVisible>,
                Changed<PickingInteraction>,
                Changed<WidgetDisabled>,
            )>,
        ),
    >,
    kinds: Query<&WidgetKind, With<WidgetOf>>,
    mut removed_focus: RemovedComponents<WidgetFocusVisible>,
    mut removed_interactions: RemovedComponents<PickingInteraction>,
    mut removed_disabled: RemovedComponents<WidgetDisabled>,
) -> bool {
    let field_removals = removed_focus
        .read()
        .chain(removed_interactions.read())
        .chain(removed_disabled.read())
        .filter(|&entity| matches!(kinds.get(entity), Ok(WidgetKind::EditableField)))
        .count();
    field_removals > 0
        || changed
            .iter()
            .any(|kind| *kind == WidgetKind::EditableField)
}

/// Maps each editable field's live state onto its retained root visual slot.
///
/// Focus reads [`WidgetFocusVisible`], hover reads the all-pointer
/// [`PickingInteraction`] aggregate, and disabled reads [`WidgetDisabled`]. A
/// field has no press, so [`WidgetState::Pressed`] never applies and its
/// element does not offer [`crate::El::pressed`].
pub(super) fn present_editable_state(
    fields: Query<
        (
            Entity,
            &WidgetKind,
            &Cascade<super::WidgetHoveredAppearance>,
            &Cascade<super::WidgetPressedAppearance>,
            &Cascade<super::WidgetFocusedAppearance>,
            &Cascade<super::WidgetDisabledAppearance>,
            &WidgetOf,
            &WidgetVisualSlots,
            Option<&PickingInteraction>,
            Has<WidgetDisabled>,
            Has<WidgetFocusVisible>,
        ),
        With<WidgetOf>,
    >,
    panels: Query<&DiegeticPanel>,
    mut overrides: Query<&mut WidgetVisualOverrides>,
    mut commands: Commands,
) {
    for (
        entity,
        kind,
        hovered,
        pressed,
        focused_appearance,
        disabled_appearance,
        widget_of,
        slots,
        interaction,
        disabled,
        focused,
    ) in &fields
    {
        if *kind != WidgetKind::EditableField {
            continue;
        }
        if slots.element_index(VisualSlotId::EDITABLE_ROOT).is_none() {
            continue;
        }
        let active = [
            focused.then_some(WidgetState::Focused),
            matches!(
                interaction,
                Some(PickingInteraction::Hovered | PickingInteraction::Pressed)
            )
            .then_some(WidgetState::Hovered),
            disabled.then_some(WidgetState::Disabled),
        ];
        let appearance =
            WidgetStateCascades::new(hovered, pressed, focused_appearance, disabled_appearance);
        let desired = appearance.resolve(&active, panels.get(widget_of.panel()).ok());
        visual::write_slot_override(
            entity,
            VisualSlotId::EDITABLE_ROOT,
            desired,
            &mut overrides,
            &mut commands,
        );
    }
}

#[cfg(test)]
mod tests {
    use bevy::ecs::system::RunSystemOnce;

    use super::*;
    use crate::Appearance;
    use crate::Border;
    use crate::CascadeEntityCommandsExt;
    use crate::ClearWidgetFocus;
    use crate::DiegeticPanel;
    use crate::El;
    use crate::HeadlessLayoutPlugin;
    use crate::ImeAppOwnedFieldSpec;
    use crate::ImeEditableFieldSpec;
    use crate::LayoutBuilder;
    use crate::Mm;
    use crate::PanelElementId;
    use crate::PanelWidgetReader;
    use crate::RequestWidgetFocus;
    use crate::WidgetInteractivity;
    use crate::text::DiegeticTextMeasurer;
    use crate::widgets::WidgetsPlugin;

    const FIELD_ID: &str = "editable";
    const FOCUS_BORDER: Color = Color::srgb(0.95, 0.85, 0.25);
    const NORMAL_BORDER: Color = Color::srgb(0.30, 0.30, 0.30);
    const NORMAL_FILL: Color = Color::srgb(0.10, 0.10, 0.12);
    const HOVER_FILL: Color = Color::srgb(0.20, 0.40, 0.80);
    const DISABLED_BORDER: Color = Color::srgb(0.35, 0.35, 0.40);

    fn test_app() -> App {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .insert_resource(DiegeticTextMeasurer::default())
            .add_plugins((HeadlessLayoutPlugin, WidgetsPlugin));
        app
    }

    fn field_tree() -> crate::LayoutTree {
        let field = ImeEditableFieldSpec::AppOwned(ImeAppOwnedFieldSpec::new("test"));
        let mut builder = LayoutBuilder::new(100.0, 50.0);
        builder.with(
            El::new()
                .border(Border::all(1.0, NORMAL_BORDER))
                .editable_field(FIELD_ID, field)
                .focused(Appearance::new().border_color(FOCUS_BORDER)),
            |_| {},
        );
        builder.build()
    }

    /// A field authoring every state layer its kind can reach: focus, hover,
    /// and disabled. A field has no press, so [`crate::El::pressed`] is not available on
    /// its element at all.
    fn state_layered_field_tree() -> crate::LayoutTree {
        let field = ImeEditableFieldSpec::AppOwned(ImeAppOwnedFieldSpec::new("test"));
        let mut builder = LayoutBuilder::new(100.0, 50.0);
        builder.with(
            El::new()
                .background(NORMAL_FILL)
                .border(Border::all(1.0, NORMAL_BORDER))
                .editable_field(FIELD_ID, field)
                .focused(Appearance::new().border_color(FOCUS_BORDER))
                .hovered(Appearance::new().background(HOVER_FILL))
                .disabled(Appearance::new().border_color(DISABLED_BORDER)),
            |_| {},
        );
        builder.build()
    }

    fn spawn_field(app: &mut App) -> Entity { spawn_field_tree(app, field_tree()) }

    fn spawn_field_tree(app: &mut App, tree: crate::LayoutTree) -> Entity {
        let panel = DiegeticPanel::world()
            .size(Mm(100.0), Mm(50.0))
            .with_tree(tree)
            .build();
        assert!(panel.is_ok());
        let panel = panel.map_or(Entity::PLACEHOLDER, |panel| {
            app.world_mut().spawn(panel).id()
        });
        app.update();
        let result = app
            .world_mut()
            .run_system_once(move |reader: PanelWidgetReader| {
                reader.entity(panel, &PanelElementId::named(FIELD_ID))
            });
        assert!(result.is_ok());
        result.ok().flatten().unwrap_or(Entity::PLACEHOLDER)
    }

    fn root_override(app: &App, widget: Entity) -> Option<VisualSlotOverride> {
        app.world()
            .get::<WidgetVisualOverrides>(widget)
            .and_then(|overrides| overrides.get(VisualSlotId::EDITABLE_ROOT).cloned())
    }

    #[test]
    fn visible_focus_presents_and_clears_the_authored_field_border() {
        let mut app = test_app();
        let window = app.world_mut().spawn(Window::default()).id();
        let field = spawn_field(&mut app);
        assert_ne!(field, Entity::PLACEHOLDER);
        assert_eq!(root_override(&app, field), None);

        app.world_mut().trigger(RequestWidgetFocus {
            window,
            widget: field,
        });
        app.world_mut().flush();
        app.update();

        assert_eq!(
            root_override(&app, field),
            Some(VisualSlotOverride {
                border_color: Some(FOCUS_BORDER),
                ..VisualSlotOverride::default()
            }),
        );

        app.world_mut().trigger(ClearWidgetFocus { window });
        app.world_mut().flush();
        app.update();

        assert_eq!(root_override(&app, field), None);
    }

    #[test]
    fn hover_presents_and_clears_the_authored_field_background() {
        let mut app = test_app();
        let field = spawn_field_tree(&mut app, state_layered_field_tree());
        assert_ne!(field, Entity::PLACEHOLDER);
        assert_eq!(root_override(&app, field), None);

        app.world_mut()
            .entity_mut(field)
            .insert(PickingInteraction::Hovered);
        app.update();

        assert_eq!(
            root_override(&app, field),
            Some(VisualSlotOverride {
                fill_color: Some(HOVER_FILL),
                ..VisualSlotOverride::default()
            }),
        );

        app.world_mut()
            .entity_mut(field)
            .remove::<PickingInteraction>();
        app.update();

        assert_eq!(root_override(&app, field), None);
    }

    #[test]
    fn disabled_presents_and_clears_the_authored_field_border() {
        let mut app = test_app();
        let field = spawn_field_tree(&mut app, state_layered_field_tree());
        assert_ne!(field, Entity::PLACEHOLDER);

        app.world_mut()
            .commands()
            .entity(field)
            .override_widget_interactivity(WidgetInteractivity::Disabled);
        app.update();

        assert_eq!(
            root_override(&app, field),
            Some(VisualSlotOverride {
                border_color: Some(DISABLED_BORDER),
                ..VisualSlotOverride::default()
            }),
        );

        app.world_mut()
            .commands()
            .entity(field)
            .override_widget_interactivity(WidgetInteractivity::Enabled);
        app.update();

        assert_eq!(root_override(&app, field), None);
    }

    #[test]
    fn focus_and_hover_layer_independently_on_a_field() {
        let mut app = test_app();
        let window = app.world_mut().spawn(Window::default()).id();
        let field = spawn_field_tree(&mut app, state_layered_field_tree());
        assert_ne!(field, Entity::PLACEHOLDER);

        app.world_mut().trigger(RequestWidgetFocus {
            window,
            widget: field,
        });
        app.world_mut().flush();
        app.world_mut()
            .entity_mut(field)
            .insert(PickingInteraction::Hovered);
        app.update();

        assert_eq!(
            root_override(&app, field),
            Some(VisualSlotOverride {
                fill_color: Some(HOVER_FILL),
                border_color: Some(FOCUS_BORDER),
                ..VisualSlotOverride::default()
            }),
            "focus authors only the border and hover only the fill, so both survive",
        );
    }
}
