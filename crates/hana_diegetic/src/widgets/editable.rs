//! Editable-field widget presentation.

use bevy::prelude::*;

use super::StateAppearance;
use super::VisualSlotId;
#[cfg(test)]
use super::VisualSlotOverride;
use super::WidgetFocusVisible;
use super::WidgetKind;
use super::WidgetOf;
use super::WidgetState;
use super::WidgetVisualOverrides;
use super::WidgetVisualSlots;
use super::visual;
use crate::DiegeticPanel;

/// Reports whether an editable field's authored focus presentation or visible
/// focus state changed.
pub(super) fn presentation_inputs_changed(
    changed: Query<
        &WidgetKind,
        (
            With<WidgetOf>,
            Or<(
                Changed<StateAppearance>,
                Changed<WidgetVisualSlots>,
                Changed<WidgetFocusVisible>,
            )>,
        ),
    >,
    kinds: Query<&WidgetKind, With<WidgetOf>>,
    mut removed_focus: RemovedComponents<WidgetFocusVisible>,
) -> bool {
    let focus_removals = removed_focus
        .read()
        .filter(|&entity| matches!(kinds.get(entity), Ok(WidgetKind::EditableField)))
        .count();
    focus_removals > 0
        || changed
            .iter()
            .any(|kind| *kind == WidgetKind::EditableField)
}

/// Maps visible keyboard focus onto each editable field's retained root slot.
pub(super) fn present_focus(
    fields: Query<
        (
            Entity,
            &WidgetKind,
            &StateAppearance,
            &WidgetOf,
            &WidgetVisualSlots,
            Has<WidgetFocusVisible>,
        ),
        With<WidgetOf>,
    >,
    panels: Query<&DiegeticPanel>,
    mut overrides: Query<&mut WidgetVisualOverrides>,
    mut commands: Commands,
) {
    for (entity, kind, appearance, widget_of, slots, focused) in &fields {
        if *kind != WidgetKind::EditableField {
            continue;
        }
        if slots.element_index(VisualSlotId::EDITABLE_ROOT).is_none() {
            continue;
        }
        let active = [focused.then_some(WidgetState::Focused)];
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
    use crate::Border;
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
    use crate::text::DiegeticTextMeasurer;
    use crate::widgets::WidgetsPlugin;

    const FIELD_ID: &str = "editable";
    const FOCUS_BORDER: Color = Color::srgb(0.95, 0.85, 0.25);
    const NORMAL_BORDER: Color = Color::srgb(0.30, 0.30, 0.30);

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
                .focused_border_color(FOCUS_BORDER),
            |_| {},
        );
        builder.build()
    }

    fn spawn_field(app: &mut App) -> Entity {
        let panel = DiegeticPanel::world()
            .size(Mm(100.0), Mm(50.0))
            .with_tree(field_tree())
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
}
