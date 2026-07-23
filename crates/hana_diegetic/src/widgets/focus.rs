use std::collections::HashMap;

use bevy::camera::NormalizedRenderTarget;
use bevy::camera::RenderTarget;
use bevy::platform::collections::HashSet;
use bevy::prelude::*;
use bevy::window::PrimaryWindow;
use bevy::window::WindowFocused;

use super::PanelWidget;
use super::PanelWidgets;
use super::WidgetOf;
use crate::panel::ComputedDiegeticPanel;

/// Marks a widget as eligible for window-scoped keyboard focus.
#[derive(Clone, Copy, Component, Debug, Eq, PartialEq, Reflect)]
#[reflect(Component)]
pub struct WidgetFocusable;

/// Marks a widget that currently owns focus in at least one window.
#[derive(Clone, Copy, Component, Debug, Eq, PartialEq, Reflect)]
#[component(immutable)]
#[reflect(Component)]
pub struct WidgetFocused(());

/// Marks a widget whose retained focus should currently be drawn.
///
/// [`WidgetFocused`] remains the keyboard-input target after a pointer press,
/// while this private marker is removed. Keyboard traversal restores this
/// marker without changing the focused widget.
#[derive(Clone, Copy, Component, Debug, Eq, PartialEq)]
#[component(immutable)]
pub(crate) struct WidgetFocusVisible(());

/// Requests focus for a live widget in one window.
#[derive(Clone, Copy, Debug, EntityEvent)]
pub struct RequestWidgetFocus {
    /// Window whose focus scope should change.
    pub window: Entity,
    /// Live widget entity that should receive focus.
    #[event_target]
    pub widget: Entity,
}

/// Clears the focused widget in one window while retaining its active panel.
#[derive(Clone, Copy, Debug, Event)]
pub struct ClearWidgetFocus {
    /// Window whose focused widget should be cleared.
    pub window: Entity,
}

/// Reports a window-scoped widget focus transition.
#[derive(Clone, Copy, Debug, Event)]
pub struct WidgetFocusChanged {
    /// Window whose focus changed.
    pub window:   Entity,
    /// Widget that previously owned focus.
    pub previous: Option<Entity>,
    /// Widget that now owns focus.
    pub current:  Option<Entity>,
    /// Reason for the transition.
    pub cause:    WidgetFocusChangeCause,
}

/// Reason for a widget focus transition.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WidgetFocusChangeCause {
    /// A pointer press selected the widget.
    Pointer,
    /// A next, previous, first, or last request selected the widget.
    Traversal,
    /// Another semantic widget operation selected the widget.
    Semantic,
    /// Application code requested the widget directly.
    Application,
    /// Application code explicitly cleared the window's focus.
    ExplicitClear,
    /// The focused widget or its owning panel role was removed.
    WidgetRemoved,
    /// The focused widget lost [`WidgetFocusable`].
    FocusabilityRemoved,
    /// The window input scope was removed.
    ScopeLost,
}

#[derive(Clone, Copy, Eq, PartialEq)]
enum FocusIndicator {
    Hidden,
    Visible,
}

#[derive(Clone, Copy)]
struct WindowFocusScope {
    active_panel: Entity,
    indicator:    FocusIndicator,
    widget:       Option<Entity>,
}

#[derive(Default, Resource)]
pub(crate) struct WidgetFocusAuthority {
    scopes: HashMap<Entity, WindowFocusScope>,
}

impl WidgetFocusAuthority {
    pub(super) fn active_panel(&self, window: Entity) -> Option<Entity> {
        self.scopes.get(&window).map(|scope| scope.active_panel)
    }

    pub(crate) fn focused_widget(&self, window: Entity) -> Option<Entity> {
        self.scopes.get(&window).and_then(|scope| scope.widget)
    }

    fn focuses(&self, widget: Entity) -> bool {
        self.scopes
            .values()
            .any(|scope| scope.widget == Some(widget))
    }

    fn shows_focus(&self, widget: Entity) -> bool {
        self.scopes
            .values()
            .any(|scope| scope.widget == Some(widget) && scope.indicator == FocusIndicator::Visible)
    }
}

#[derive(Clone, Copy)]
pub(super) enum WidgetTraversal {
    Next,
    Previous,
    First,
    Last,
}

pub(super) fn request_widget_focus(
    request: On<RequestWidgetFocus>,
    windows: Query<(), With<Window>>,
    widgets: Query<&WidgetOf, (With<PanelWidget>, With<WidgetFocusable>)>,
    mut authority: ResMut<WidgetFocusAuthority>,
    mut commands: Commands,
) {
    let request = request.event();
    if windows.get(request.window).is_err() {
        return;
    }
    let Ok(widget_of) = widgets.get(request.widget) else {
        return;
    };
    transition_focus(
        &mut authority,
        request.window,
        Some(WindowFocusScope {
            active_panel: widget_of.panel(),
            indicator:    FocusIndicator::Visible,
            widget:       Some(request.widget),
        }),
        WidgetFocusChangeCause::Application,
        &mut commands,
    );
}

pub(super) fn clear_widget_focus(
    request: On<ClearWidgetFocus>,
    mut authority: ResMut<WidgetFocusAuthority>,
    mut commands: Commands,
) {
    let request = request.event();
    let next = authority
        .scopes
        .get(&request.window)
        .map(|scope| WindowFocusScope {
            active_panel: scope.active_panel,
            indicator:    FocusIndicator::Hidden,
            widget:       None,
        });
    transition_focus(
        &mut authority,
        request.window,
        next,
        WidgetFocusChangeCause::ExplicitClear,
        &mut commands,
    );
}

pub(super) fn focus_from_pointer_press(
    press: On<Pointer<Press>>,
    cameras: Query<&RenderTarget, With<Camera>>,
    primary_window: Query<Entity, With<PrimaryWindow>>,
    widgets: Query<&WidgetOf, (With<PanelWidget>, With<WidgetFocusable>)>,
    windows: Query<(), With<Window>>,
    mut authority: ResMut<WidgetFocusAuthority>,
    mut commands: Commands,
) {
    if press.button != PointerButton::Primary {
        return;
    }
    let Some(window) = hit_camera_window(press.hit.camera, &cameras, &primary_window) else {
        return;
    };
    if windows.get(window).is_err() {
        return;
    }
    let widget = press.event_target();
    let Ok(widget_of) = widgets.get(widget) else {
        return;
    };
    transition_focus(
        &mut authority,
        window,
        Some(WindowFocusScope {
            active_panel: widget_of.panel(),
            indicator:    FocusIndicator::Hidden,
            widget:       Some(widget),
        }),
        WidgetFocusChangeCause::Pointer,
        &mut commands,
    );
}

pub(super) fn cleanup_removed_focus_participants(
    mut removed_focusable: RemovedComponents<WidgetFocusable>,
    mut removed_widgets: RemovedComponents<PanelWidget>,
    mut removed_windows: RemovedComponents<Window>,
    mut window_focus_events: MessageReader<WindowFocused>,
    participants: Query<
        (Has<PanelWidget>, Has<WidgetFocusable>),
        Or<(With<PanelWidget>, With<WidgetFocusable>)>,
    >,
    windows: Query<(), With<Window>>,
    mut authority: ResMut<WidgetFocusAuthority>,
    mut commands: Commands,
) {
    let removed_participants = removed_focusable
        .read()
        .chain(removed_widgets.read())
        .collect::<HashSet<_>>();
    for widget in removed_participants {
        let state = participants.get(widget).ok();
        if state.is_some_and(|(is_widget, is_focusable)| is_widget && is_focusable) {
            continue;
        }
        let cause = if state.is_some_and(|(is_widget, _)| is_widget) {
            WidgetFocusChangeCause::FocusabilityRemoved
        } else {
            WidgetFocusChangeCause::WidgetRemoved
        };
        clear_removed_widget_focus(widget, cause, &mut authority, &mut commands);
    }

    let removed_scopes = removed_windows.read().collect::<Vec<_>>();
    for window in removed_scopes {
        if windows.get(window).is_ok() {
            continue;
        }
        transition_focus(
            &mut authority,
            window,
            None,
            WidgetFocusChangeCause::ScopeLost,
            &mut commands,
        );
    }

    for event in window_focus_events.read() {
        if event.focused {
            continue;
        }
        transition_focus(
            &mut authority,
            event.window,
            None,
            WidgetFocusChangeCause::ScopeLost,
            &mut commands,
        );
    }
}

pub(super) fn traverse_focus(
    window: Entity,
    traversal: WidgetTraversal,
    authority: &mut WidgetFocusAuthority,
    panels: &Query<(&ComputedDiegeticPanel, &PanelWidgets)>,
    widgets: &Query<(&PanelWidget, &WidgetOf), With<WidgetFocusable>>,
    commands: &mut Commands<'_, '_>,
) {
    let Some(panel) = authority.active_panel(window) else {
        return;
    };
    let Ok((computed, panel_widgets)) = panels.get(panel) else {
        return;
    };
    let widgets_by_id = panel_widgets
        .iter()
        .filter_map(|widget| {
            let (panel_widget, widget_of) = widgets.get(widget).ok()?;
            (widget_of.panel() == panel).then_some((panel_widget.id(), widget))
        })
        .collect::<HashMap<_, _>>();
    let mut ordered = computed
        .widget_records()
        .iter()
        .filter_map(|record| {
            widgets_by_id
                .get(record.id())
                .copied()
                .map(|widget| (record.preorder(), widget))
        })
        .collect::<Vec<_>>();
    ordered.sort_by_key(|(preorder, _)| *preorder);
    let Some(target_index) = traversal_index(
        traversal,
        authority.focused_widget(window),
        ordered.as_slice(),
    ) else {
        return;
    };
    let target = ordered[target_index].1;
    transition_focus(
        authority,
        window,
        Some(WindowFocusScope {
            active_panel: panel,
            indicator:    FocusIndicator::Visible,
            widget:       Some(target),
        }),
        WidgetFocusChangeCause::Traversal,
        commands,
    );
}

pub(crate) fn finalize_panel_focus(
    panel: Entity,
    authority: &mut WidgetFocusAuthority,
    commands: &mut Commands<'_, '_>,
) {
    let affected_windows = authority
        .scopes
        .iter()
        .filter_map(|(&window, scope)| (scope.active_panel == panel).then_some(window))
        .collect::<Vec<_>>();
    for window in affected_windows {
        transition_focus(
            authority,
            window,
            None,
            WidgetFocusChangeCause::WidgetRemoved,
            commands,
        );
    }
}

fn hit_camera_window(
    camera: Entity,
    cameras: &Query<&RenderTarget, With<Camera>>,
    primary_window: &Query<Entity, With<PrimaryWindow>>,
) -> Option<Entity> {
    match cameras
        .get(camera)
        .ok()?
        .normalize(primary_window.single().ok())?
    {
        NormalizedRenderTarget::Window(window_ref) => Some(window_ref.entity()),
        NormalizedRenderTarget::Image(_)
        | NormalizedRenderTarget::TextureView(_)
        | NormalizedRenderTarget::None { .. } => None,
    }
}

fn clear_removed_widget_focus(
    widget: Entity,
    cause: WidgetFocusChangeCause,
    authority: &mut WidgetFocusAuthority,
    commands: &mut Commands<'_, '_>,
) {
    let affected = authority
        .scopes
        .iter()
        .filter_map(|(&window, scope)| {
            (scope.widget == Some(widget)).then_some((window, scope.active_panel))
        })
        .collect::<Vec<_>>();
    for (window, active_panel) in affected {
        transition_focus(
            authority,
            window,
            Some(WindowFocusScope {
                active_panel,
                indicator: FocusIndicator::Hidden,
                widget: None,
            }),
            cause,
            commands,
        );
    }
}

fn traversal_index(
    traversal: WidgetTraversal,
    focused: Option<Entity>,
    ordered: &[(usize, Entity)],
) -> Option<usize> {
    if ordered.is_empty() {
        return None;
    }
    let focused_index = focused.and_then(|focused| {
        ordered
            .iter()
            .position(|(_, candidate)| *candidate == focused)
    });
    match traversal {
        WidgetTraversal::First => Some(0),
        WidgetTraversal::Last => Some(ordered.len() - 1),
        WidgetTraversal::Next => Some(focused_index.map_or(0, |index| (index + 1) % ordered.len())),
        WidgetTraversal::Previous => Some(focused_index.map_or(ordered.len() - 1, |index| {
            index.checked_sub(1).unwrap_or(ordered.len() - 1)
        })),
    }
}

fn transition_focus(
    authority: &mut WidgetFocusAuthority,
    window: Entity,
    next: Option<WindowFocusScope>,
    cause: WidgetFocusChangeCause,
    commands: &mut Commands<'_, '_>,
) {
    let previous = authority.focused_widget(window);
    let next_widget = next.and_then(|scope| scope.widget);
    let affected_widgets = [previous, next_widget]
        .into_iter()
        .flatten()
        .collect::<HashSet<_>>();
    let previous_markers = affected_widgets
        .iter()
        .map(|&widget| {
            (
                widget,
                authority.focuses(widget),
                authority.shows_focus(widget),
            )
        })
        .collect::<Vec<_>>();
    match next {
        Some(scope) => {
            authority.scopes.insert(window, scope);
        },
        None => {
            authority.scopes.remove(&window);
        },
    }
    let current = authority.focused_widget(window);
    for (widget, was_focused, was_visible) in previous_markers {
        let is_focused = authority.focuses(widget);
        let is_visible = authority.shows_focus(widget);
        if was_focused != is_focused {
            if is_focused {
                commands.entity(widget).insert(WidgetFocused(()));
            } else {
                commands.entity(widget).try_remove::<WidgetFocused>();
            }
        }
        if was_visible != is_visible {
            if is_visible {
                commands.entity(widget).insert(WidgetFocusVisible(()));
            } else {
                commands.entity(widget).try_remove::<WidgetFocusVisible>();
            }
        }
    }
    if previous == current {
        return;
    }
    commands.trigger(WidgetFocusChanged {
        window,
        previous,
        current,
        cause,
    });
}

#[cfg(test)]
mod tests {
    use bevy::camera::NormalizedRenderTarget;
    use bevy::camera::RenderTarget;
    use bevy::ecs::system::RunSystemOnce;
    use bevy::picking::backend::HitData;
    use bevy::picking::pointer::Location;
    use bevy::picking::pointer::PointerId;
    use bevy::prelude::*;
    use bevy::window::PrimaryWindow;
    use bevy::window::WindowFocused;
    use bevy::window::WindowRef;
    use hana_valence::AnchorId;
    use hana_valence::AnchoredHere;
    use hana_valence::AnchoredTo;

    use super::ClearWidgetFocus;
    use super::RequestWidgetFocus;
    use super::WidgetFocusAuthority;
    use super::WidgetFocusChangeCause;
    use super::WidgetFocusChanged;
    use super::WidgetFocusVisible;
    use super::WidgetFocusable;
    use super::WidgetFocused;
    use crate::Button;
    use crate::DiegeticPanel;
    use crate::DiegeticPanelCommands;
    use crate::El;
    use crate::FocusFirstWidget;
    use crate::FocusLastWidget;
    use crate::FocusNextWidget;
    use crate::FocusPreviousWidget;
    use crate::HeadlessLayoutPlugin;
    use crate::LayoutBuilder;
    use crate::LayoutTree;
    use crate::Mm;
    use crate::PanelElementId;
    use crate::PanelWidgetReader;
    use crate::WidgetOf;
    use crate::text::DiegeticTextMeasurer;
    use crate::widgets::PanelWidget;
    use crate::widgets::ScreenWidgetAnchoredHere;
    use crate::widgets::ScreenWidgetAnchoredTo;
    use crate::widgets::WidgetsPlugin;

    const PANEL_HEIGHT: f32 = 50.0;
    const PANEL_WIDTH: f32 = 100.0;

    #[derive(Default, Resource)]
    struct FocusChanges(Vec<WidgetFocusChanged>);

    #[derive(Default, Resource)]
    struct TeardownObservation {
        changes:   usize,
        relations: Option<TeardownRelations>,
    }

    struct TeardownRelations;

    fn record_focus_change(change: On<WidgetFocusChanged>, mut changes: ResMut<FocusChanges>) {
        changes.0.push(*change.event());
    }

    fn observe_teardown_change(
        change: On<WidgetFocusChanged>,
        widgets: Query<(&WidgetOf, &AnchoredHere, &ScreenWidgetAnchoredHere), With<PanelWidget>>,
        mut observation: ResMut<TeardownObservation>,
    ) {
        if change.cause != WidgetFocusChangeCause::WidgetRemoved {
            return;
        }
        observation.changes += 1;
        let Some(previous) = change.previous else {
            return;
        };
        if widgets.get(previous).is_ok() {
            observation.relations = Some(TeardownRelations);
        }
    }

    fn test_app() -> App {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .insert_resource(DiegeticTextMeasurer::default())
            .init_resource::<FocusChanges>()
            .add_plugins((HeadlessLayoutPlugin, WidgetsPlugin))
            .add_observer(record_focus_change);
        app
    }

    fn widget_tree(ids: &[&str]) -> LayoutTree {
        let mut builder = LayoutBuilder::new(PANEL_WIDTH, PANEL_HEIGHT);
        for id in ids {
            builder.with(El::new().button(*id, Button::new()), |_| {});
        }
        builder.build()
    }

    fn spawn_panel(app: &mut App, ids: &[&str]) -> Entity {
        let result = DiegeticPanel::world()
            .size(Mm(PANEL_WIDTH), Mm(PANEL_HEIGHT))
            .with_tree(widget_tree(ids))
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

    fn focused(app: &App, window: Entity) -> Option<Entity> {
        app.world()
            .resource::<WidgetFocusAuthority>()
            .focused_widget(window)
    }

    fn request_focus(app: &mut App, window: Entity, widget: Entity) {
        app.world_mut()
            .trigger(RequestWidgetFocus { window, widget });
        app.world_mut().flush();
    }

    #[test]
    fn traversal_uses_record_preorder_and_wraps() {
        let mut app = test_app();
        let window = app.world_mut().spawn(Window::default()).id();
        let panel = spawn_panel(&mut app, &["first", "second", "third"]);
        app.update();
        let first = widget(&mut app, panel, "first");
        let second = widget(&mut app, panel, "second");
        let third = widget(&mut app, panel, "third");
        request_focus(&mut app, window, second);

        app.world_mut().write_message(FocusNextWidget { window });
        app.update();
        assert_eq!(focused(&app, window), Some(third));

        app.world_mut().write_message(FocusNextWidget { window });
        app.update();
        assert_eq!(focused(&app, window), Some(first));

        app.world_mut()
            .write_message(FocusPreviousWidget { window });
        app.update();
        assert_eq!(focused(&app, window), Some(third));

        app.world_mut().write_message(FocusFirstWidget { window });
        app.update();
        assert_eq!(focused(&app, window), Some(first));

        app.world_mut().write_message(FocusLastWidget { window });
        app.update();
        assert_eq!(focused(&app, window), Some(third));
    }

    #[test]
    fn reorder_changes_traversal_without_respawning_widgets() {
        let mut app = test_app();
        let window = app.world_mut().spawn(Window::default()).id();
        let panel = spawn_panel(&mut app, &["first", "second", "third"]);
        app.update();
        let first = widget(&mut app, panel, "first");
        let second = widget(&mut app, panel, "second");
        let third = widget(&mut app, panel, "third");
        request_focus(&mut app, window, third);

        let result = app
            .world_mut()
            .commands()
            .set_tree(panel, widget_tree(&["third", "first", "second"]));
        assert!(result.is_ok());
        app.update();

        assert_eq!(widget(&mut app, panel, "first"), first);
        assert_eq!(widget(&mut app, panel, "second"), second);
        assert_eq!(widget(&mut app, panel, "third"), third);
        app.world_mut().write_message(FocusNextWidget { window });
        app.update();
        assert_eq!(focused(&app, window), Some(first));
    }

    #[test]
    fn windows_keep_independent_focus_and_shared_marker_state() {
        let mut app = test_app();
        let first_window = app.world_mut().spawn(Window::default()).id();
        let second_window = app.world_mut().spawn(Window::default()).id();
        let panel = spawn_panel(&mut app, &["first", "second"]);
        app.update();
        let first = widget(&mut app, panel, "first");
        let second = widget(&mut app, panel, "second");

        request_focus(&mut app, first_window, first);
        request_focus(&mut app, second_window, second);
        assert_eq!(focused(&app, first_window), Some(first));
        assert_eq!(focused(&app, second_window), Some(second));
        assert!(app.world().get::<WidgetFocused>(first).is_some());
        assert!(app.world().get::<WidgetFocused>(second).is_some());

        request_focus(&mut app, second_window, first);
        app.world_mut().trigger(ClearWidgetFocus {
            window: first_window,
        });
        app.world_mut().flush();
        assert_eq!(focused(&app, first_window), None);
        assert_eq!(focused(&app, second_window), Some(first));
        assert!(app.world().get::<WidgetFocused>(first).is_some());

        let changes = &app.world().resource::<FocusChanges>().0;
        assert_eq!(
            changes.last().map(|change| change.cause),
            Some(WidgetFocusChangeCause::ExplicitClear)
        );
    }

    #[test]
    fn removed_focusable_stays_removed_across_reify_and_clears_focus() {
        let mut app = test_app();
        let window = app.world_mut().spawn(Window::default()).id();
        let panel = spawn_panel(&mut app, &["first", "second"]);
        app.update();
        let first = widget(&mut app, panel, "first");
        request_focus(&mut app, window, first);

        app.world_mut()
            .entity_mut(first)
            .remove::<WidgetFocusable>();
        app.update();
        assert_eq!(focused(&app, window), None);
        assert_eq!(
            app.world()
                .resource::<FocusChanges>()
                .0
                .last()
                .map(|change| change.cause),
            Some(WidgetFocusChangeCause::FocusabilityRemoved)
        );

        let result = app
            .world_mut()
            .commands()
            .set_tree(panel, widget_tree(&["second", "first"]));
        assert!(result.is_ok());
        app.update();
        assert_eq!(widget(&mut app, panel, "first"), first);
        assert!(app.world().get::<WidgetFocusable>(first).is_none());
    }

    #[test]
    fn window_input_scope_loss_clears_only_that_windows_focus() {
        let mut app = test_app();
        let lost_window = app.world_mut().spawn(Window::default()).id();
        let retained_window = app.world_mut().spawn(Window::default()).id();
        let panel = spawn_panel(&mut app, &["target"]);
        app.update();
        let target = widget(&mut app, panel, "target");
        request_focus(&mut app, lost_window, target);
        request_focus(&mut app, retained_window, target);

        app.world_mut().write_message(WindowFocused {
            window:  lost_window,
            focused: false,
        });
        app.update();

        assert_eq!(focused(&app, lost_window), None);
        assert_eq!(focused(&app, retained_window), Some(target));
        assert!(app.world().get::<WidgetFocused>(target).is_some());
        assert_eq!(
            app.world()
                .resource::<FocusChanges>()
                .0
                .last()
                .map(|change| change.cause),
            Some(WidgetFocusChangeCause::ScopeLost)
        );
    }

    #[test]
    fn pointer_focus_uses_the_hit_camera_render_target() {
        let mut app = test_app();
        let primary_window = app
            .world_mut()
            .spawn((Window::default(), PrimaryWindow))
            .id();
        let pointer_window = app.world_mut().spawn(Window::default()).id();
        let panel = spawn_panel(&mut app, &["remembered", "target"]);
        app.update();
        let remembered = widget(&mut app, panel, "remembered");
        let target = widget(&mut app, panel, "target");
        request_focus(&mut app, primary_window, remembered);
        assert!(app.world().get::<WidgetFocusVisible>(remembered).is_some());
        let none_camera = app
            .world_mut()
            .spawn((Camera::default(), RenderTarget::None { size: UVec2::ONE }))
            .id();
        let primary_camera = app
            .world_mut()
            .spawn((Camera::default(), RenderTarget::Window(WindowRef::Primary)))
            .id();
        let Some(primary_window_ref) = WindowRef::Entity(primary_window).normalize(None) else {
            return;
        };
        let Some(pointer_window_ref) = WindowRef::Entity(pointer_window).normalize(None) else {
            return;
        };

        app.world_mut().trigger(Pointer::new(
            PointerId::Mouse,
            Location {
                target:   NormalizedRenderTarget::Window(primary_window_ref),
                position: Vec2::ZERO,
            },
            Press {
                button: PointerButton::Primary,
                hit:    HitData::new(none_camera, 0.0, None, None),
                count:  1,
            },
            target,
        ));
        app.world_mut().flush();
        assert_eq!(focused(&app, primary_window), Some(remembered));

        app.world_mut().trigger(Pointer::new(
            PointerId::Mouse,
            Location {
                target:   NormalizedRenderTarget::Window(pointer_window_ref),
                position: Vec2::ZERO,
            },
            Press {
                button: PointerButton::Primary,
                hit:    HitData::new(primary_camera, 0.0, None, None),
                count:  1,
            },
            target,
        ));
        app.world_mut().flush();
        assert_eq!(focused(&app, primary_window), Some(target));
        assert_eq!(focused(&app, pointer_window), None);
        assert!(app.world().get::<WidgetFocused>(target).is_some());
        assert!(app.world().get::<WidgetFocusVisible>(target).is_none());
        assert_eq!(
            app.world()
                .resource::<FocusChanges>()
                .0
                .last()
                .map(|change| change.cause),
            Some(WidgetFocusChangeCause::Pointer)
        );

        app.world_mut().resource_mut::<FocusChanges>().0.clear();
        app.world_mut().write_message(FocusLastWidget {
            window: primary_window,
        });
        app.update();

        assert_eq!(focused(&app, primary_window), Some(target));
        assert!(app.world().get::<WidgetFocused>(target).is_some());
        assert!(app.world().get::<WidgetFocusVisible>(target).is_some());
        assert!(
            app.world().resource::<FocusChanges>().0.is_empty(),
            "revealing the indicator must not report a semantic focus transition",
        );
    }

    #[test]
    fn direct_focused_widget_despawn_clears_authority_once() {
        let mut app = test_app();
        let window = app.world_mut().spawn(Window::default()).id();
        let panel = spawn_panel(&mut app, &["target"]);
        app.update();
        let target = widget(&mut app, panel, "target");
        request_focus(&mut app, window, target);
        app.world_mut().resource_mut::<FocusChanges>().0.clear();

        assert!(app.world_mut().despawn(target));
        app.update();
        app.update();

        assert_eq!(focused(&app, window), None);
        let changes = &app.world().resource::<FocusChanges>().0;
        assert_eq!(changes.len(), 1);
        assert_eq!(changes[0].window, window);
        assert_eq!(changes[0].previous, Some(target));
        assert_eq!(changes[0].current, None);
        assert_eq!(changes[0].cause, WidgetFocusChangeCause::WidgetRemoved);
    }

    #[test]
    fn panel_role_teardown_reports_focus_before_widget_relations_are_removed() {
        let mut app = test_app();
        app.init_resource::<TeardownObservation>()
            .add_observer(observe_teardown_change);
        let window = app.world_mut().spawn(Window::default()).id();
        let panel = spawn_panel(&mut app, &["target"]);
        app.update();
        let target = widget(&mut app, panel, "target");
        request_focus(&mut app, window, target);

        app.world_mut()
            .spawn(AnchoredTo::new(target, AnchorId::Center, AnchorId::Center));
        app.world_mut().spawn(ScreenWidgetAnchoredTo::new(target));
        assert!(app.world().get::<AnchoredHere>(target).is_some());
        assert!(
            app.world()
                .get::<ScreenWidgetAnchoredHere>(target)
                .is_some()
        );

        app.world_mut().entity_mut(panel).remove::<DiegeticPanel>();

        let observation = app.world().resource::<TeardownObservation>();
        assert_eq!(observation.changes, 1);
        assert!(observation.relations.is_some());
        assert_eq!(focused(&app, window), None);
        assert!(app.world().get::<WidgetFocused>(target).is_none());
    }

    #[test]
    fn full_panel_despawn_reports_focus_before_widget_relations_are_removed() {
        let mut app = test_app();
        app.init_resource::<TeardownObservation>()
            .add_observer(observe_teardown_change);
        let window = app.world_mut().spawn(Window::default()).id();
        let panel = spawn_panel(&mut app, &["target"]);
        app.update();
        let target = widget(&mut app, panel, "target");
        request_focus(&mut app, window, target);

        app.world_mut()
            .spawn(AnchoredTo::new(target, AnchorId::Center, AnchorId::Center));
        app.world_mut().spawn(ScreenWidgetAnchoredTo::new(target));
        assert!(app.world().get::<AnchoredHere>(target).is_some());
        assert!(
            app.world()
                .get::<ScreenWidgetAnchoredHere>(target)
                .is_some()
        );
        app.world_mut().resource_mut::<FocusChanges>().0.clear();

        assert!(app.world_mut().despawn(panel));

        let observation = app.world().resource::<TeardownObservation>();
        assert_eq!(observation.changes, 1);
        assert!(observation.relations.is_some());
        assert_eq!(focused(&app, window), None);
        let changes = &app.world().resource::<FocusChanges>().0;
        assert_eq!(changes.len(), 1);
        assert_eq!(changes[0].window, window);
        assert_eq!(changes[0].previous, Some(target));
        assert_eq!(changes[0].current, None);
        assert_eq!(changes[0].cause, WidgetFocusChangeCause::WidgetRemoved);
    }
}
