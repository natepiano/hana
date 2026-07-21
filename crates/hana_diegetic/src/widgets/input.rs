use bevy::ecs::system::SystemParam;
use bevy::prelude::*;

use super::PanelWidget;
use super::PanelWidgets;
use super::WidgetDisabled;
use super::WidgetFocusAuthority;
use super::WidgetFocusable;
use super::WidgetOf;
use super::focus;
use super::focus::WidgetTraversal;
use crate::ime::ImeInputBlocker;
use crate::panel::ComputedDiegeticPanel;

/// Requests focus on the next widget in the active panel for a window.
#[derive(Clone, Copy, Debug, Message)]
pub struct FocusNextWidget {
    /// Window whose active panel should be traversed.
    pub window: Entity,
}

/// Requests focus on the previous widget in the active panel for a window.
#[derive(Clone, Copy, Debug, Message)]
pub struct FocusPreviousWidget {
    /// Window whose active panel should be traversed.
    pub window: Entity,
}

/// Requests focus on the first widget in the active panel for a window.
#[derive(Clone, Copy, Debug, Message)]
pub struct FocusFirstWidget {
    /// Window whose active panel should be traversed.
    pub window: Entity,
}

/// Requests focus on the last widget in the active panel for a window.
#[derive(Clone, Copy, Debug, Message)]
pub struct FocusLastWidget {
    /// Window whose active panel should be traversed.
    pub window: Entity,
}

/// Requests activation of the focused widget in a window.
#[derive(Clone, Copy, Debug, Message)]
pub struct ActivateFocusedWidget {
    /// Window whose focused widget should receive activation.
    pub window: Entity,
}

/// Requests cancellation of the focused widget in a window.
#[derive(Clone, Copy, Debug, Message)]
pub struct CancelFocusedWidget {
    /// Window whose focused widget should receive cancellation.
    pub window: Entity,
}

#[derive(Clone, Copy, Debug, Event)]
#[event(trigger = bevy::ecs::event::EntityTrigger)]
pub(crate) enum SemanticWidgetIntent {
    Activate { entity: Entity },
    Cancel { entity: Entity },
}

impl EntityEvent for SemanticWidgetIntent {
    fn event_target(&self) -> Entity {
        match *self {
            Self::Activate { entity } | Self::Cancel { entity } => entity,
        }
    }
}

#[derive(SystemParam)]
pub(super) struct SemanticInputReaders<'w, 's> {
    next:     MessageReader<'w, 's, FocusNextWidget>,
    previous: MessageReader<'w, 's, FocusPreviousWidget>,
    first:    MessageReader<'w, 's, FocusFirstWidget>,
    last:     MessageReader<'w, 's, FocusLastWidget>,
    activate: MessageReader<'w, 's, ActivateFocusedWidget>,
    cancel:   MessageReader<'w, 's, CancelFocusedWidget>,
}

pub(super) fn route_semantic_input(
    mut readers: SemanticInputReaders,
    input_blocker: Option<Res<ImeInputBlocker>>,
    mut authority: ResMut<WidgetFocusAuthority>,
    panels: Query<(&ComputedDiegeticPanel, &PanelWidgets)>,
    focusable_widgets: Query<(&PanelWidget, &WidgetOf), With<WidgetFocusable>>,
    enabled_widgets: Query<(), (With<PanelWidget>, Without<WidgetDisabled>)>,
    mut commands: Commands,
) {
    for request in readers.next.read() {
        route_traversal(
            request.window,
            WidgetTraversal::Next,
            input_blocker.as_deref(),
            &mut authority,
            &panels,
            &focusable_widgets,
            &mut commands,
        );
    }
    for request in readers.previous.read() {
        route_traversal(
            request.window,
            WidgetTraversal::Previous,
            input_blocker.as_deref(),
            &mut authority,
            &panels,
            &focusable_widgets,
            &mut commands,
        );
    }
    for request in readers.first.read() {
        route_traversal(
            request.window,
            WidgetTraversal::First,
            input_blocker.as_deref(),
            &mut authority,
            &panels,
            &focusable_widgets,
            &mut commands,
        );
    }
    for request in readers.last.read() {
        route_traversal(
            request.window,
            WidgetTraversal::Last,
            input_blocker.as_deref(),
            &mut authority,
            &panels,
            &focusable_widgets,
            &mut commands,
        );
    }
    for request in readers.activate.read() {
        route_activate_intent(
            request.window,
            input_blocker.as_deref(),
            &authority,
            &enabled_widgets,
            &mut commands,
        );
    }
    for request in readers.cancel.read() {
        route_cancel_intent(
            request.window,
            input_blocker.as_deref(),
            &authority,
            &enabled_widgets,
            &mut commands,
        );
    }
}

fn route_traversal(
    window: Entity,
    traversal: WidgetTraversal,
    input_blocker: Option<&ImeInputBlocker>,
    authority: &mut WidgetFocusAuthority,
    panels: &Query<(&ComputedDiegeticPanel, &PanelWidgets)>,
    focusable_widgets: &Query<(&PanelWidget, &WidgetOf), With<WidgetFocusable>>,
    commands: &mut Commands<'_, '_>,
) {
    if input_blocker.is_some_and(|blocker| blocker.blocks_window(window)) {
        return;
    }
    focus::traverse_focus(
        window,
        traversal,
        authority,
        panels,
        focusable_widgets,
        commands,
    );
}

fn route_activate_intent(
    window: Entity,
    input_blocker: Option<&ImeInputBlocker>,
    authority: &WidgetFocusAuthority,
    enabled_widgets: &Query<(), (With<PanelWidget>, Without<WidgetDisabled>)>,
    commands: &mut Commands<'_, '_>,
) {
    if input_blocker.is_some_and(|blocker| blocker.blocks_window(window)) {
        return;
    }
    let Some(widget) = authority.focused_widget(window) else {
        return;
    };
    if enabled_widgets.get(widget).is_err() {
        return;
    }
    commands.trigger(SemanticWidgetIntent::Activate { entity: widget });
}

fn route_cancel_intent(
    window: Entity,
    input_blocker: Option<&ImeInputBlocker>,
    authority: &WidgetFocusAuthority,
    enabled_widgets: &Query<(), (With<PanelWidget>, Without<WidgetDisabled>)>,
    commands: &mut Commands<'_, '_>,
) {
    if input_blocker.is_some_and(|blocker| blocker.blocks_window(window)) {
        return;
    }
    let Some(widget) = authority.focused_widget(window) else {
        return;
    };
    if enabled_widgets.get(widget).is_err() {
        return;
    }
    commands.trigger(SemanticWidgetIntent::Cancel { entity: widget });
}

#[cfg(test)]
mod tests {
    use bevy::camera::NormalizedRenderTarget;
    use bevy::camera::RenderTarget;
    use bevy::ecs::system::RunSystemOnce;
    use bevy::input::keyboard::KeyboardInput;
    use bevy::picking::backend::HitData;
    use bevy::picking::pointer::Location;
    use bevy::picking::pointer::PointerId;
    use bevy::prelude::*;
    use bevy::window::Ime;
    use bevy::window::WindowClosed;
    use bevy::window::WindowFocused;
    use bevy::window::WindowRef;

    use super::ActivateFocusedWidget;
    use super::CancelFocusedWidget;
    use super::SemanticWidgetIntent;
    use crate::Button;
    use crate::DiegeticPanel;
    use crate::El;
    use crate::HeadlessLayoutPlugin;
    use crate::ImeAppOwnedFieldSpec;
    use crate::ImeEditableFieldSpec;
    use crate::ImeOpenSession;
    use crate::ImeTarget;
    use crate::LayoutBuilder;
    use crate::Mm;
    use crate::PanelElementId;
    use crate::PanelWidgetReader;
    use crate::PanelWidgetWriter;
    use crate::RequestWidgetFocus;
    use crate::WidgetDisabled;
    use crate::WidgetFocused;
    use crate::WidgetInteractivity;
    use crate::ime::ImePlugin;
    use crate::text::DiegeticTextMeasurer;
    use crate::widgets::WidgetsPlugin;

    const PANEL_HEIGHT: f32 = 50.0;
    const PANEL_WIDTH: f32 = 100.0;

    #[derive(Clone, Copy)]
    enum TestIme {
        Absent,
        Present,
    }

    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    enum RecordedIntentAction {
        Activate,
        Cancel,
    }

    #[derive(Default, Resource)]
    struct RecordedIntents(Vec<(Entity, RecordedIntentAction)>);

    fn record_intent(intent: On<SemanticWidgetIntent>, mut recorded: ResMut<RecordedIntents>) {
        let entry = match *intent.event() {
            SemanticWidgetIntent::Activate { entity } => (entity, RecordedIntentAction::Activate),
            SemanticWidgetIntent::Cancel { entity } => (entity, RecordedIntentAction::Cancel),
        };
        recorded.0.push(entry);
    }

    fn test_app(test_ime: TestIme) -> App {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .insert_resource(DiegeticTextMeasurer::default())
            .init_resource::<RecordedIntents>()
            .add_plugins((HeadlessLayoutPlugin, WidgetsPlugin))
            .add_observer(record_intent);
        if matches!(test_ime, TestIme::Present) {
            app.add_message::<Ime>()
                .add_message::<KeyboardInput>()
                .add_message::<WindowClosed>()
                .add_message::<WindowFocused>()
                .init_resource::<ButtonInput<KeyCode>>()
                .add_plugins(ImePlugin);
        }
        app
    }

    fn spawn_panel(app: &mut App, ids: &[&str]) -> Entity {
        let mut builder = LayoutBuilder::new(PANEL_WIDTH, PANEL_HEIGHT);
        for id in ids {
            builder.with(El::new().button(*id, Button::new()), |_| {});
        }
        let result = DiegeticPanel::world()
            .size(Mm(PANEL_WIDTH), Mm(PANEL_HEIGHT))
            .with_tree(builder.build())
            .build();
        assert!(result.is_ok());
        let Ok(panel) = result else {
            return Entity::PLACEHOLDER;
        };
        app.world_mut().spawn(panel).id()
    }

    fn widget(app: &mut App, panel: Entity, id: &'static str) -> Entity {
        let id = PanelElementId::named(id);
        let result = app
            .world_mut()
            .run_system_once(move |reader: PanelWidgetReader| reader.entity(panel, &id));
        assert!(result.is_ok());
        let Ok(widget) = result else {
            return Entity::PLACEHOLDER;
        };
        assert!(widget.is_some());
        widget.unwrap_or(Entity::PLACEHOLDER)
    }

    fn request_focus(app: &mut App, window: Entity, widget: Entity) {
        app.world_mut()
            .trigger(RequestWidgetFocus { window, widget });
    }

    #[test]
    fn pointer_focus_routes_each_semantic_intent_once() {
        let mut app = test_app(TestIme::Absent);
        let window = app.world_mut().spawn(Window::default()).id();
        let camera = app
            .world_mut()
            .spawn((
                Camera::default(),
                RenderTarget::Window(WindowRef::Entity(window)),
            ))
            .id();
        let panel = spawn_panel(&mut app, &["target"]);
        app.update();
        let target = widget(&mut app, panel, "target");
        let Some(window_ref) = WindowRef::Entity(window).normalize(None) else {
            return;
        };
        app.world_mut().trigger(Pointer::new(
            PointerId::Mouse,
            Location {
                target:   NormalizedRenderTarget::Window(window_ref),
                position: Vec2::ZERO,
            },
            Press {
                button: PointerButton::Primary,
                hit:    HitData::new(camera, 0.0, None, None),
                count:  1,
            },
            target,
        ));
        app.world_mut()
            .write_message(ActivateFocusedWidget { window });
        app.world_mut()
            .write_message(CancelFocusedWidget { window });
        app.update();

        assert_eq!(
            app.world().resource::<RecordedIntents>().0,
            vec![
                (target, RecordedIntentAction::Activate),
                (target, RecordedIntentAction::Cancel),
            ]
        );
    }

    #[test]
    fn same_frame_disabled_resolution_suppresses_semantic_intent_without_clearing_focus() {
        let mut app = test_app(TestIme::Absent);
        let window = app.world_mut().spawn(Window::default()).id();
        let panel = spawn_panel(&mut app, &["target"]);
        app.update();
        let target = widget(&mut app, panel, "target");
        request_focus(&mut app, window, target);

        let result = app
            .world_mut()
            .run_system_once(move |mut writer: PanelWidgetWriter| {
                writer.override_interactivity(target, WidgetInteractivity::Disabled)
            });
        assert!(matches!(result, Ok(true)));
        app.world_mut()
            .write_message(ActivateFocusedWidget { window });
        app.update();

        assert!(app.world().get::<WidgetDisabled>(target).is_some());
        assert!(app.world().get::<WidgetFocused>(target).is_some());
        assert!(app.world().resource::<RecordedIntents>().0.is_empty());
    }

    #[test]
    fn ime_blocks_only_the_leased_windows_semantic_input() {
        let mut app = test_app(TestIme::Present);
        let blocked_window = app.world_mut().spawn(Window::default()).id();
        let available_window = app.world_mut().spawn(Window::default()).id();
        let panel = spawn_panel(&mut app, &["blocked", "available"]);
        app.update();
        let blocked = widget(&mut app, panel, "blocked");
        let available = widget(&mut app, panel, "available");
        request_focus(&mut app, blocked_window, blocked);
        request_focus(&mut app, available_window, available);

        let owner = app.world_mut().spawn_empty().id();
        app.world_mut().trigger(ImeOpenSession {
            target:       ImeTarget::AppOwned {
                owner,
                field_id: PanelElementId::named("ime-owner"),
            },
            window:       blocked_window,
            initial_text: String::new(),
            field_spec:   ImeEditableFieldSpec::AppOwned(ImeAppOwnedFieldSpec::new("test")),
            anchor:       None,
        });
        app.world_mut().write_message(ActivateFocusedWidget {
            window: blocked_window,
        });
        app.world_mut().write_message(ActivateFocusedWidget {
            window: available_window,
        });
        app.update();

        assert_eq!(
            app.world().resource::<RecordedIntents>().0,
            vec![(available, RecordedIntentAction::Activate)]
        );
    }
}
