//! Canonical runtime example for `hana_diegetic` widgets.
//!
//! The example keeps the complete widget interaction path runnable in one
//! Fairy Dust app.
//!
//! Current controls:
//!   D - Toggle the secondary button between enabled and disabled
//!   H - Return to the camera home pose
//!   Tab / Shift+Tab - Focus the next/previous widget through Hana's adapter
//!   Home / End - Focus the first/last widget through Hana's adapter
//!   Enter or Space / Escape - Activate/cancel through Hana's adapter
//!   P - Focus the previous widget through an app-owned Bevy Kana action

use bevy::picking::hover::PickingInteraction;
use bevy::prelude::*;
use bevy::window::PrimaryWindow;
use bevy_enhanced_input::prelude::ActionSettings;
use bevy_enhanced_input::prelude::ActionSpawner;
use bevy_enhanced_input::prelude::Actions;
use bevy_enhanced_input::prelude::InputAction;
use bevy_enhanced_input::prelude::InputContextAppExt;
use bevy_kana::Keybindings;
use bevy_kana::action;
use bevy_kana::bind_action_system;
use bevy_kana::event;
use bevy_lagrange::OrbitCamPreset;
use fairy_dust::CameraHomeTarget;
use fairy_dust::ControlActivation;
use fairy_dust::DescriptionPanel;
use fairy_dust::Face;
use fairy_dust::FairyDustCube;
use fairy_dust::TitleBar;
use fairy_dust::cube_face_panel_material;
use fairy_dust::cube_face_transform;
use hana_diegetic::AlignX;
use hana_diegetic::AlignY;
use hana_diegetic::Anchor;
use hana_diegetic::Border;
use hana_diegetic::Button;
use hana_diegetic::ButtonCanceled;
use hana_diegetic::ButtonClicked;
use hana_diegetic::ButtonPressed;
use hana_diegetic::ButtonReleased;
use hana_diegetic::CornerRadius;
use hana_diegetic::DiegeticPanel;
use hana_diegetic::DiegeticPanelCommands;
use hana_diegetic::El;
use hana_diegetic::FacePicking;
use hana_diegetic::FitMax;
use hana_diegetic::FocusPreviousWidget;
use hana_diegetic::LayoutBuilder;
use hana_diegetic::LayoutTree;
use hana_diegetic::Padding;
use hana_diegetic::PanelAnchorOffset;
use hana_diegetic::PanelAttachment;
use hana_diegetic::PanelElementId;
use hana_diegetic::PanelEntityReader;
use hana_diegetic::PanelPicking;
use hana_diegetic::PanelText;
use hana_diegetic::PanelWidget;
use hana_diegetic::PanelWidgetReader;
use hana_diegetic::PanelWidgetWriter;
use hana_diegetic::Px;
use hana_diegetic::RequestWidgetFocus;
use hana_diegetic::Sizing;
use hana_diegetic::Slider;
use hana_diegetic::SliderConfigError;
use hana_diegetic::SliderDirection;
use hana_diegetic::SliderRange;
use hana_diegetic::SliderStep;
use hana_diegetic::Text;
use hana_diegetic::TextStyle;
use hana_diegetic::WidgetDisabled;
use hana_diegetic::WidgetFocusChanged;
use hana_diegetic::WidgetFocused;
use hana_diegetic::WidgetInputPlugin;
use hana_diegetic::WidgetInteractivity;
use hana_diegetic::WidgetOf;

// widget lab
const BUTTON_BORDER: Color = Color::srgba(0.30, 0.62, 1.0, 0.82);
const BUTTON_BORDER_DISABLED: Color = Color::srgba(0.34, 0.36, 0.40, 0.60);
const BUTTON_BORDER_FOCUSED: Color = Color::srgba(1.0, 0.86, 0.30, 0.94);
const BUTTON_FILL: Color = Color::srgba(0.03, 0.10, 0.24, 0.82);
const BUTTON_FILL_DISABLED: Color = Color::srgba(0.10, 0.11, 0.13, 0.66);
const BUTTON_FILL_HOVERED: Color = Color::srgba(0.10, 0.26, 0.52, 0.88);
const BUTTON_FILL_PRESSED: Color = Color::srgba(0.55, 0.30, 0.08, 0.94);
const BUTTON_HEIGHT: Px = Px(42.0);
const CONTROL_BORDER_WIDTH: Px = Px(1.0);
const CONTROL_GAP: Px = Px(8.0);
const CONTROL_PADDING: Px = Px(8.0);
const CONTROL_RADIUS: Px = Px(7.0);
const CONTROL_TEXT: Color = Color::srgb(0.92, 0.96, 1.0);
const CONTROL_WIDTH: Px = Px(280.0);
const CUBE_CLEARANCE: f32 = 0.1;
const DESCRIPTION_LINES: [&str; 6] = [
    "Buttons restyle on hover, press, and focus; interaction changes log in the terminal.",
    "D disables the secondary button, which then shows its disabled surface.",
    "Tab controls use Hana's adapter; P sends the same request from an app-owned action.",
    "The primary button's on_click callback counts clicks in the status readout.",
    "The world status panel follows the level slider below the cube controls.",
    "The screen status panel follows the separate top-right screen widget.",
];
const PANEL_BACKGROUND: Color = Color::srgba(0.02, 0.03, 0.07, 0.92);
const PANEL_BORDER: Color = Color::srgba(0.05, 0.60, 0.86, 0.86);
const PANEL_BORDER_WIDTH: Px = Px(2.0);
const PANEL_FACE_OFFSET: f32 = 0.012;
const PANEL_MAX_HEIGHT: Px = Px(240.0);
const PANEL_MAX_WIDTH: Px = Px(340.0);
const PANEL_PADDING: Px = Px(12.0);
const PANEL_RADIUS: Px = Px(10.0);
const PANEL_TITLE: &str = "Widget Lab";
const PANEL_WORLD_HEIGHT: f32 = 0.32;
const BUTTON_STATUS_ID: &str = "button-status";
const BUTTON_STATUS_IDLE: &str = "Button: none";
const BUTTON_STATUS_MEASURE: &str = "Button: Canceled secondary-button (pointer/cause)";
const CALLBACK_STATUS_ID: &str = "callback-status";
const CALLBACK_STATUS_IDLE: &str = "Callback: none";
const CALLBACK_STATUS_MEASURE: &str = "Callback: 999 clicks on primary-button";
const FOCUS_STATUS_ID: &str = "focus-status";
const FOCUS_STATUS_MEASURE: &str = "Focus: secondary-button";
const FOCUS_STATUS_NONE: &str = "Focus: none";
const FOCUS_STATUS_UNAVAILABLE: &str = "Focus: unavailable";
const PRIMARY_BUTTON_ID: &str = "primary-button";
const POINTER_STATUS_ID: &str = "pointer-status";
const POINTER_STATUS_IDLE: &str = "Pointer: none";
const POINTER_STATUS_MEASURE: &str = "Pointer: Pressed secondary-button";
const SECONDARY_BUTTON_ID: &str = "secondary-button";
const SECONDARY_CONTROL: &str = "D Toggle Secondary";
const SCREEN_CONTROL_WIDTH: Px = Px(218.0);
const SCREEN_PANEL_MAX_HEIGHT: Px = Px(120.0);
const SCREEN_PANEL_MAX_WIDTH: Px = Px(250.0);
const SCREEN_READOUT_MAX_HEIGHT: Px = Px(64.0);
const SCREEN_READOUT_MAX_WIDTH: Px = Px(260.0);
const SCREEN_READOUT_COLOR: Color = Color::srgb(1.0, 0.78, 0.32);
const SCREEN_READOUT_ID: &str = "screen-anchor-status";
const SCREEN_READOUT_TEXT: &str = "^ Attached panel";
const SCREEN_TARGET_ID: &str = "screen-target-button";
const SCREEN_TARGET_LABEL: &str = "Target widget";
const STATE_STATUS_ID: &str = "state-status";
const STATE_STATUS_IDLE: &str = "State: pri=normal sec=normal";
const STATE_STATUS_MEASURE: &str = "State: pri=hover,focus,off sec=hover,focus,off";
const SLIDER_BORDER: Color = Color::srgba(0.62, 0.46, 1.0, 0.82);
const SLIDER_FILL: Color = Color::srgba(0.12, 0.04, 0.26, 0.82);
const SLIDER_HEIGHT: Px = Px(36.0);
const SLIDER_ID: &str = "level-slider";
const SLIDER_INITIAL_VALUE: f32 = 0.5;
const SLIDER_RANGE_END: f32 = 1.0;
const SLIDER_RANGE_START: f32 = 0.0;
const SLIDER_STEP: f32 = 0.05;
const STATUS_BACKGROUND: Color = Color::srgba(0.01, 0.06, 0.08, 0.88);
const STATUS_ANCHOR_OFFSET: Px = Px(12.0);
const STATUS_BORDER: Color = Color::srgba(0.20, 0.80, 0.68, 0.86);
const STATUS_BORDER_WIDTH: Px = Px(1.0);
const STATUS_COLOR: Color = Color::srgb(0.38, 0.94, 0.78);
const STATUS_GAP: f32 = 0.012;
const STATUS_LINE_GAP: Px = Px(4.0);
const STATUS_PADDING: Px = Px(6.0);
const STATUS_RADIUS: Px = Px(7.0);
const WORLD_READOUT_MAX_HEIGHT: Px = Px(116.0);
const WORLD_READOUT_MAX_WIDTH: Px = Px(420.0);
const WORLD_READOUT_WORLD_HEIGHT: f32 = 0.12;

#[derive(Clone, Copy, Default, Resource)]
enum SecondaryMode {
    #[default]
    Enabled,
    Disabled,
}

impl SecondaryMode {
    const fn toggled(self) -> Self {
        match self {
            Self::Enabled => Self::Disabled,
            Self::Disabled => Self::Enabled,
        }
    }

    const fn interactivity(self) -> WidgetInteractivity {
        match self {
            Self::Enabled => WidgetInteractivity::Enabled,
            Self::Disabled => WidgetInteractivity::Disabled,
        }
    }

    const fn control_activation(self) -> ControlActivation {
        match self {
            Self::Enabled => ControlActivation::Inactive,
            Self::Disabled => ControlActivation::Active,
        }
    }
}

#[derive(Clone, Copy, Default)]
enum InteractionChanges {
    #[default]
    None,
    Observed,
}

impl InteractionChanges {
    const fn observe(&mut self) { *self = Self::Observed; }

    const fn were_observed(self) -> bool { matches!(self, Self::Observed) }
}

#[derive(Clone, Copy, Default, Eq, Ord, PartialEq, PartialOrd)]
enum InteractionPriority {
    #[default]
    None,
    Hovered,
    Pressed,
}

impl From<PickingInteraction> for InteractionPriority {
    fn from(interaction: PickingInteraction) -> Self {
        match interaction {
            PickingInteraction::None => Self::None,
            PickingInteraction::Hovered => Self::Hovered,
            PickingInteraction::Pressed => Self::Pressed,
        }
    }
}

#[derive(Default, Resource)]
struct PrimaryClicks(usize);

#[derive(Component)]
struct WidgetLabPanel;

#[derive(Component)]
struct WidgetInteractionReadout;

#[derive(Component)]
struct WidgetAnchorInstalled;

#[derive(Component)]
struct InitialWidgetFocusRequested;

#[derive(Component)]
struct ScreenWidgetLabPanel;

#[derive(Component)]
struct ScreenWidgetInteractionReadout;

#[derive(Component)]
struct ScreenWidgetAnchorInstalled;

#[derive(Component)]
struct AppWidgetInputContext;

action!(
    /// App-owned action that requests previous-widget focus.
    AppFocusPrevious
);

action!(
    /// Modifier action used by the app-owned widget keybindings.
    AppWidgetShift
);

event!(
    /// App-owned event that invokes the core widget-focus request system.
    AppFocusPreviousEvent
);

struct AppOwnedWidgetInputPlugin;

impl Plugin for AppOwnedWidgetInputPlugin {
    fn build(&self, app: &mut App) {
        app.add_input_context::<AppWidgetInputContext>()
            .add_systems(Startup, spawn_app_widget_input);
        bind_action_system!(
            app,
            AppFocusPrevious,
            AppFocusPreviousEvent,
            focus_previous_widget
        );
    }
}

fn main() {
    // `hana_diegetic::DiegeticUiPlugin` is registered automatically by
    // `fairy_dust::sprinkle_example`.
    fairy_dust::sprinkle_example()
        .with_brp_extras()
        .with_save_window_position()
        .with_studio_lighting()
        .with_ground_plane()
        .with_cube()
        .size(fairy_dust::EXAMPLE_CUBE_SIZE)
        .color(fairy_dust::EXAMPLE_CUBE_COLOR)
        .transform(Transform::from_translation(
            fairy_dust::example_cube_on_ground(CUBE_CLEARANCE),
        ))
        .insert(CameraHomeTarget)
        .with_orbit_cam_preset(|_| {}, OrbitCamPreset::blender_like())
        .with_stable_transparency()
        .with_camera_home()
        .with_title_bar(
            TitleBar::new()
                .with_title("Widgets")
                .with_anchor(Anchor::TopLeft)
                .control(SECONDARY_CONTROL),
        )
        .wire_chip_to_state::<SecondaryMode, _>(SECONDARY_CONTROL, |mode| mode.control_activation())
        .with_description_panel(
            DescriptionPanel::new(PANEL_TITLE)
                .with_anchor(Anchor::BottomLeft)
                .lines(DESCRIPTION_LINES),
        )
        .with_camera_control_panel()
        .init_resource::<SecondaryMode>()
        .init_resource::<PrimaryClicks>()
        .add_plugins((WidgetInputPlugin, AppOwnedWidgetInputPlugin))
        .add_observer(report_button_pressed)
        .add_observer(report_button_released)
        .add_observer(report_button_clicked)
        .add_observer(report_button_canceled)
        .add_observer(report_widget_focus_changed)
        .add_systems(PostStartup, spawn_widget_lab)
        .add_systems(
            Update,
            (
                anchor_interaction_readout,
                anchor_screen_interaction_readout,
                report_interaction_changes,
                report_presentation_states,
                request_initial_widget_focus,
            ),
        )
        .with_shortcut(KeyCode::KeyD, toggle_secondary_button)
        .run();
}

fn report_button_pressed(
    event: On<ButtonPressed>,
    readouts: Query<Entity, With<WidgetInteractionReadout>>,
    mut panel_text: PanelText,
) {
    retain_button_status(
        &readouts,
        &mut panel_text,
        format!("Button: Pressed {} ({:?})", event.id, event.pointer_id),
    );
    info!("widgets: {} pressed by {:?}", event.id, event.pointer_id);
}

fn report_button_released(
    event: On<ButtonReleased>,
    readouts: Query<Entity, With<WidgetInteractionReadout>>,
    mut panel_text: PanelText,
) {
    retain_button_status(
        &readouts,
        &mut panel_text,
        format!("Button: Released {} ({:?})", event.id, event.pointer_id),
    );
    info!("widgets: {} released by {:?}", event.id, event.pointer_id);
}

fn report_button_clicked(
    event: On<ButtonClicked>,
    readouts: Query<Entity, With<WidgetInteractionReadout>>,
    mut panel_text: PanelText,
) {
    let status = event.pointer_id.as_ref().map_or_else(
        || format!("Button: Clicked {} (semantic)", event.id),
        |pointer_id| format!("Button: Clicked {} ({pointer_id:?})", event.id),
    );
    retain_button_status(&readouts, &mut panel_text, status);
    info!("widgets: {} clicked by {:?}", event.id, event.pointer_id);
}

fn report_button_canceled(
    event: On<ButtonCanceled>,
    readouts: Query<Entity, With<WidgetInteractionReadout>>,
    mut panel_text: PanelText,
) {
    retain_button_status(
        &readouts,
        &mut panel_text,
        format!(
            "Button: Canceled {} ({:?}, {:?})",
            event.id, event.pointer_id, event.cause
        ),
    );
    info!(
        "widgets: {} canceled for {:?} by {:?}",
        event.id, event.cause, event.pointer_id
    );
}

/// Typed `.on_click` callback for the primary button.
///
/// Installed through `Button::new().on_click(...)` at authoring time; reify
/// registers it once as a tracked system and the plugin's single
/// global `ButtonClicked` observer dispatches it with each completed click.
fn count_primary_click(
    click: In<ButtonClicked>,
    mut clicks: ResMut<PrimaryClicks>,
    readouts: Query<Entity, With<WidgetInteractionReadout>>,
    mut panel_text: PanelText,
) {
    clicks.0 += 1;
    info!(
        "widgets: on_click callback ran for {} ({:?}), {} total",
        click.id, click.pointer_id, clicks.0
    );
    let Ok(readout) = readouts.single() else {
        return;
    };
    let status = format!("Callback: {} clicks on {}", clicks.0, click.id);
    if !panel_text.set_text(readout, &PanelElementId::named(CALLBACK_STATUS_ID), status) {
        warn!("widgets: callback status has not been reified");
    }
}

fn retain_button_status(
    readouts: &Query<Entity, With<WidgetInteractionReadout>>,
    panel_text: &mut PanelText,
    status: String,
) {
    let Ok(readout) = readouts.single() else {
        return;
    };
    if !panel_text.set_text(readout, &PanelElementId::named(BUTTON_STATUS_ID), status) {
        warn!("widgets: button status has not been reified");
    }
}

fn spawn_app_widget_input(mut commands: Commands) {
    commands.spawn((
        AppWidgetInputContext,
        Actions::<AppWidgetInputContext>::spawn(SpawnWith(spawn_app_widget_actions)),
    ));
}

fn spawn_app_widget_actions(spawner: &mut ActionSpawner<AppWidgetInputContext>) {
    let keybindings = Keybindings::new::<AppWidgetShift>(spawner, ActionSettings::default());
    keybindings.spawn_key::<AppFocusPrevious>(spawner, KeyCode::KeyP);
}

fn request_initial_widget_focus(
    mut commands: Commands,
    panel: Single<Entity, (With<WidgetLabPanel>, Without<InitialWidgetFocusRequested>)>,
    window: Single<Entity, With<PrimaryWindow>>,
    reader: PanelWidgetReader,
) {
    let id = PanelElementId::named(PRIMARY_BUTTON_ID);
    let Some(widget) = reader.entity(*panel, &id) else {
        return;
    };
    commands.trigger(RequestWidgetFocus {
        window: *window,
        widget,
    });
    commands.entity(*panel).insert(InitialWidgetFocusRequested);
}

fn report_widget_focus_changed(
    change: On<WidgetFocusChanged>,
    readouts: Query<Entity, With<WidgetInteractionReadout>>,
    widgets: Query<&PanelWidget>,
    mut panel_text: PanelText,
) {
    info!(
        "widgets: window {:?} focus changed from {:?} to {:?} ({:?})",
        change.window, change.previous, change.current, change.cause
    );

    let focus_status = match change.current {
        Some(entity) => widgets.get(entity).map_or_else(
            |_| FOCUS_STATUS_UNAVAILABLE.to_owned(),
            |widget| format!("Focus: {}", widget.id()),
        ),
        None => FOCUS_STATUS_NONE.to_owned(),
    };
    let Ok(readout) = readouts.single() else {
        return;
    };
    if !panel_text.set_text(
        readout,
        &PanelElementId::named(FOCUS_STATUS_ID),
        focus_status,
    ) {
        warn!("widgets: focus status has not been reified");
    }
}

fn focus_previous_widget(
    window: Single<Entity, With<PrimaryWindow>>,
    mut requests: MessageWriter<FocusPreviousWidget>,
) {
    requests.write(FocusPreviousWidget { window: *window });
}

fn anchor_screen_interaction_readout(
    mut commands: Commands,
    panel: Single<Entity, With<ScreenWidgetLabPanel>>,
    readout: Single<
        Entity,
        (
            With<ScreenWidgetInteractionReadout>,
            Without<ScreenWidgetAnchorInstalled>,
        ),
    >,
    reader: PanelWidgetReader,
    panel_entities: PanelEntityReader,
) {
    let id = PanelElementId::named(SCREEN_TARGET_ID);
    let Some(owner) = panel_entities.screen(*panel) else {
        return;
    };
    let Some(source) = panel_entities.screen(*readout) else {
        return;
    };
    let Some(widget) = reader.typed_entity(owner, &id) else {
        return;
    };
    let authored = PanelAttachment::new(Anchor::TopRight, Anchor::BottomRight)
        .with_offset(PanelAnchorOffset::new(Px(0.0), STATUS_ANCHOR_OFFSET));
    commands.attach_to_widget(source, widget, authored);
    commands
        .entity(*readout)
        .insert(ScreenWidgetAnchorInstalled);
}

fn anchor_interaction_readout(
    mut commands: Commands,
    panel: Single<Entity, With<WidgetLabPanel>>,
    readout: Single<
        Entity,
        (
            With<WidgetInteractionReadout>,
            Without<WidgetAnchorInstalled>,
        ),
    >,
    reader: PanelWidgetReader,
    panel_entities: PanelEntityReader,
) {
    let id = PanelElementId::named(SLIDER_ID);
    let Some(owner) = panel_entities.world(*panel) else {
        return;
    };
    let Some(source) = panel_entities.world(*readout) else {
        return;
    };
    let Some(widget) = reader.typed_entity(owner, &id) else {
        return;
    };
    let authored = PanelAttachment::new(Anchor::TopCenter, Anchor::BottomCenter)
        .with_offset(PanelAnchorOffset::new(Px(0.0), STATUS_ANCHOR_OFFSET));
    commands.attach_to_widget(source, widget, authored);
    commands.entity(*readout).insert(WidgetAnchorInstalled);
}

fn spawn_widget_lab(
    mut commands: Commands,
    mut materials: ResMut<Assets<StandardMaterial>>,
    cube: Single<Entity, With<FairyDustCube>>,
) {
    let slider = match slider_declaration() {
        Ok(slider) => slider,
        Err(error) => {
            error!("widgets: failed to construct slider: {error}");
            return;
        },
    };
    let material = materials.add(cube_face_panel_material());
    let panel = DiegeticPanel::world()
        .size(
            FitMax(PANEL_MAX_WIDTH.into()),
            FitMax(PANEL_MAX_HEIGHT.into()),
        )
        .world_height(PANEL_WORLD_HEIGHT)
        .anchor(Anchor::Center)
        .picking(PanelPicking {
            front: FacePicking::Interactive,
            back:  FacePicking::PanelOnly,
        })
        .material(material.clone())
        .text_material(material.clone())
        .with_tree(widget_tree(slider))
        .build();
    let readout = DiegeticPanel::world()
        .size(
            FitMax(WORLD_READOUT_MAX_WIDTH.into()),
            FitMax(WORLD_READOUT_MAX_HEIGHT.into()),
        )
        .world_height(WORLD_READOUT_WORLD_HEIGHT)
        .anchor(Anchor::Center)
        .material(material.clone())
        .text_material(material.clone())
        .with_tree(interaction_status_tree())
        .build();
    let screen_panel = DiegeticPanel::screen()
        .size(
            FitMax(SCREEN_PANEL_MAX_WIDTH.into()),
            FitMax(SCREEN_PANEL_MAX_HEIGHT.into()),
        )
        .anchor(Anchor::TopRight)
        .material(material.clone())
        .text_material(material.clone())
        .with_tree(screen_widget_tree())
        .build();
    let screen_readout = DiegeticPanel::screen()
        .size(
            FitMax(SCREEN_READOUT_MAX_WIDTH.into()),
            FitMax(SCREEN_READOUT_MAX_HEIGHT.into()),
        )
        .anchor(Anchor::TopRight)
        .material(material.clone())
        .text_material(material)
        .with_tree(screen_interaction_status_tree())
        .build();

    match panel {
        Ok(panel) => {
            commands.entity(*cube).with_children(|cube| {
                cube.spawn((
                    Name::new("Widget lab panel"),
                    WidgetLabPanel,
                    panel,
                    widget_panel_transform(),
                ));
                match readout {
                    Ok(readout) => {
                        cube.spawn((
                            Name::new("Widget interaction readout"),
                            WidgetInteractionReadout,
                            readout,
                            interaction_status_transform(),
                        ));
                    },
                    Err(error) => error!("widgets: failed to build interaction readout: {error}"),
                }
            });
        },
        Err(error) => error!("widgets: failed to build widget panel: {error}"),
    }

    match (screen_panel, screen_readout) {
        (Ok(screen_panel), Ok(screen_readout)) => {
            commands.spawn((
                Name::new("Screen widget target panel"),
                ScreenWidgetLabPanel,
                screen_panel,
            ));
            commands.spawn((
                Name::new("Screen widget attachment readout"),
                ScreenWidgetInteractionReadout,
                screen_readout,
            ));
        },
        (Err(error), _) => error!("widgets: failed to build screen widget panel: {error}"),
        (_, Err(error)) => error!("widgets: failed to build screen widget readout: {error}"),
    }
}

fn screen_widget_tree() -> LayoutTree {
    let mut builder = LayoutBuilder::with_root(
        El::column()
            .width(Sizing::FIT)
            .height(Sizing::FIT)
            .padding(Padding::all(PANEL_PADDING))
            .gap(CONTROL_GAP)
            .background(PANEL_BACKGROUND)
            .border(Border::all(PANEL_BORDER_WIDTH, SCREEN_READOUT_COLOR))
            .corner_radius(CornerRadius::all(PANEL_RADIUS)),
    );
    builder.text((
        "Screen anchoring",
        TextStyle::new(fairy_dust::LABEL_SIZE).with_color(SCREEN_READOUT_COLOR),
    ));
    add_button(
        &mut builder,
        SCREEN_TARGET_ID,
        SCREEN_TARGET_LABEL,
        SCREEN_CONTROL_WIDTH,
        Button::new(),
    );
    builder.build()
}

fn screen_interaction_status_tree() -> LayoutTree {
    let mut builder = LayoutBuilder::with_root(
        El::new()
            .width(Sizing::FIT)
            .height(Sizing::FIT)
            .padding(Padding::all(STATUS_PADDING))
            .alignment(AlignX::Center, AlignY::Center)
            .background(STATUS_BACKGROUND)
            .border(Border::all(STATUS_BORDER_WIDTH, SCREEN_READOUT_COLOR))
            .corner_radius(CornerRadius::all(STATUS_RADIUS)),
    );
    builder.text(
        Text::new(
            SCREEN_READOUT_TEXT,
            TextStyle::new(fairy_dust::LABEL_SIZE).with_color(SCREEN_READOUT_COLOR),
        )
        .id(SCREEN_READOUT_ID),
    );
    builder.build()
}

fn slider_declaration() -> Result<Slider, SliderConfigError> {
    let range = SliderRange::new(SLIDER_RANGE_START, SLIDER_RANGE_END)?;
    let step = SliderStep::new(SLIDER_STEP)?;
    Ok(Slider::new(range, SLIDER_INITIAL_VALUE)?
        .step(step)
        .direction(SliderDirection::LeftToRight))
}

fn widget_tree(slider: Slider) -> LayoutTree {
    let mut builder = LayoutBuilder::with_root(
        El::column()
            .width(Sizing::FIT)
            .height(Sizing::FIT)
            .padding(Padding::all(PANEL_PADDING))
            .gap(CONTROL_GAP)
            .background(PANEL_BACKGROUND)
            .border(Border::all(PANEL_BORDER_WIDTH, PANEL_BORDER))
            .corner_radius(CornerRadius::all(PANEL_RADIUS)),
    );
    builder.text((
        PANEL_TITLE,
        TextStyle::new(fairy_dust::TITLE_SIZE).with_color(fairy_dust::TITLE_COLOR),
    ));
    add_button(
        &mut builder,
        PRIMARY_BUTTON_ID,
        "Primary button",
        CONTROL_WIDTH,
        state_styled(Button::new().on_click(count_primary_click)),
    );
    add_button(
        &mut builder,
        SECONDARY_BUTTON_ID,
        "Secondary button",
        CONTROL_WIDTH,
        state_styled(Button::new())
            .disabled_background(BUTTON_FILL_DISABLED)
            .disabled_border_color(BUTTON_BORDER_DISABLED),
    );
    builder.with(
        El::new()
            .size(CONTROL_WIDTH, SLIDER_HEIGHT)
            .padding(Padding::all(CONTROL_PADDING))
            .alignment(AlignX::Center, AlignY::Center)
            .background(SLIDER_FILL)
            .border(Border::all(CONTROL_BORDER_WIDTH, SLIDER_BORDER))
            .corner_radius(CornerRadius::all(CONTROL_RADIUS))
            .slider(SLIDER_ID, slider),
        |builder| {
            builder.text((
                "Level slider — 50%",
                TextStyle::new(fairy_dust::LABEL_SIZE).with_color(CONTROL_TEXT),
            ));
        },
    );
    builder.build()
}

fn interaction_status_tree() -> LayoutTree {
    let mut builder = LayoutBuilder::with_root(
        El::column()
            .width(Sizing::FIT)
            .height(Sizing::FIT)
            .padding(Padding::all(STATUS_PADDING))
            .gap(STATUS_LINE_GAP)
            .alignment(AlignX::Left, AlignY::Center)
            .background(STATUS_BACKGROUND)
            .border(Border::all(STATUS_BORDER_WIDTH, STATUS_BORDER))
            .corner_radius(CornerRadius::all(STATUS_RADIUS)),
    );
    builder.text(
        Text::new(
            POINTER_STATUS_IDLE,
            TextStyle::new(fairy_dust::LABEL_SIZE).with_color(STATUS_COLOR),
        )
        .id(POINTER_STATUS_ID)
        .measure_as(POINTER_STATUS_MEASURE),
    );
    builder.text(
        Text::new(
            FOCUS_STATUS_NONE,
            TextStyle::new(fairy_dust::LABEL_SIZE).with_color(STATUS_COLOR),
        )
        .id(FOCUS_STATUS_ID)
        .measure_as(FOCUS_STATUS_MEASURE),
    );
    builder.text(
        Text::new(
            BUTTON_STATUS_IDLE,
            TextStyle::new(fairy_dust::LABEL_SIZE).with_color(STATUS_COLOR),
        )
        .id(BUTTON_STATUS_ID)
        .measure_as(BUTTON_STATUS_MEASURE),
    );
    builder.text(
        Text::new(
            CALLBACK_STATUS_IDLE,
            TextStyle::new(fairy_dust::LABEL_SIZE).with_color(STATUS_COLOR),
        )
        .id(CALLBACK_STATUS_ID)
        .measure_as(CALLBACK_STATUS_MEASURE),
    );
    builder.text(
        Text::new(
            STATE_STATUS_IDLE,
            TextStyle::new(fairy_dust::LABEL_SIZE).with_color(STATUS_COLOR),
        )
        .id(STATE_STATUS_ID)
        .measure_as(STATE_STATUS_MEASURE),
    );
    builder.build()
}

fn widget_panel_transform() -> Transform {
    let mut transform = cube_face_transform(Face::Front, fairy_dust::EXAMPLE_CUBE_SIZE);
    transform.translation += transform.rotation * Vec3::Z * PANEL_FACE_OFFSET;
    transform
}

fn interaction_status_transform() -> Transform {
    let mut transform = widget_panel_transform();
    transform.translation.y -=
        (PANEL_WORLD_HEIGHT + WORLD_READOUT_WORLD_HEIGHT).mul_add(0.5, STATUS_GAP);
    transform
}

/// Direct state presentation shared by the world-panel buttons: hover and
/// press restyle the fill while focus restyles only the border, so the two
/// properties visibly layer independently.
fn state_styled(button: Button) -> Button {
    button
        .hovered_background(BUTTON_FILL_HOVERED)
        .pressed_background(BUTTON_FILL_PRESSED)
        .focused_border_color(BUTTON_BORDER_FOCUSED)
}

fn add_button(
    builder: &mut LayoutBuilder,
    id: &'static str,
    label: &'static str,
    width: Px,
    button: Button,
) {
    builder.with(
        El::new()
            .size(width, BUTTON_HEIGHT)
            .padding(Padding::all(CONTROL_PADDING))
            .alignment(AlignX::Center, AlignY::Center)
            .background(BUTTON_FILL)
            .border(Border::all(CONTROL_BORDER_WIDTH, BUTTON_BORDER))
            .corner_radius(CornerRadius::all(CONTROL_RADIUS))
            .button(id, button),
        |builder| {
            builder.text((
                label,
                TextStyle::new(fairy_dust::LABEL_SIZE).with_color(CONTROL_TEXT),
            ));
        },
    );
}

fn report_interaction_changes(
    panel: Single<Entity, With<WidgetLabPanel>>,
    screen_panel: Single<Entity, With<ScreenWidgetLabPanel>>,
    readout: Single<Entity, With<WidgetInteractionReadout>>,
    widgets: Query<(&PanelWidget, &WidgetOf, Ref<PickingInteraction>)>,
    mut panel_text: PanelText,
) {
    let mut world_interaction_changes = InteractionChanges::None;
    let mut active_priority = InteractionPriority::None;
    let mut active_status = None;

    for (widget, widget_of, interaction) in &widgets {
        let owner = widget_of.panel();
        if owner != *panel && owner != *screen_panel {
            continue;
        }
        let interaction_changed = interaction.is_changed();
        if interaction_changed {
            info!(
                "widgets: {} interaction changed to {:?}",
                widget.id(),
                *interaction
            );
        }
        if owner != *panel {
            continue;
        }
        if interaction_changed {
            world_interaction_changes.observe();
        }

        let priority = InteractionPriority::from(*interaction);
        if priority > active_priority {
            active_priority = priority;
            active_status = Some(format!("Pointer: {:?} {}", *interaction, widget.id()));
        }
    }

    if world_interaction_changes.were_observed()
        && !panel_text.set_text(
            *readout,
            &PanelElementId::named(POINTER_STATUS_ID),
            active_status.as_deref().unwrap_or(POINTER_STATUS_IDLE),
        )
    {
        warn!("widgets: interaction status has not been reified");
    }
}

/// Mirrors each world button's app-visible presentation inputs into the
/// `State:` diagnostic row. Hover comes from `PickingInteraction`, focus from
/// `WidgetFocused`, and disabled from `WidgetDisabled`; press already appears
/// on the `Button:` row through its events.
fn report_presentation_states(
    panel: Single<Entity, With<WidgetLabPanel>>,
    readout: Single<Entity, With<WidgetInteractionReadout>>,
    widgets: Query<(
        &PanelWidget,
        &WidgetOf,
        Option<&PickingInteraction>,
        Has<WidgetFocused>,
        Has<WidgetDisabled>,
    )>,
    mut panel_text: PanelText,
) {
    let flags = |id: &str| {
        widgets
            .iter()
            .find(|(widget, widget_of, ..)| {
                widget_of.panel() == *panel && *widget.id() == PanelElementId::named(id)
            })
            .map_or_else(
                || "?".to_owned(),
                |(_, _, interaction, focused, disabled)| {
                    let mut parts = Vec::new();
                    if matches!(
                        interaction,
                        Some(PickingInteraction::Hovered | PickingInteraction::Pressed)
                    ) {
                        parts.push("hover");
                    }
                    if focused {
                        parts.push("focus");
                    }
                    if disabled {
                        parts.push("off");
                    }
                    if parts.is_empty() {
                        "normal".to_owned()
                    } else {
                        parts.join(",")
                    }
                },
            )
    };
    let status = format!(
        "State: pri={} sec={}",
        flags(PRIMARY_BUTTON_ID),
        flags(SECONDARY_BUTTON_ID)
    );
    // `PanelText` skips the layout revision bump for an unchanged string, so
    // writing every frame stays free of relayout work.
    panel_text.set_text(*readout, &PanelElementId::named(STATE_STATUS_ID), status);
}

fn toggle_secondary_button(
    panel: Single<Entity, With<WidgetLabPanel>>,
    reader: PanelWidgetReader,
    mut writer: PanelWidgetWriter,
    mut mode: ResMut<SecondaryMode>,
) {
    let id = PanelElementId::named(SECONDARY_BUTTON_ID);
    let Some(widget) = reader.entity(*panel, &id) else {
        warn!("widgets: secondary button has not been reified");
        return;
    };
    let next = mode.toggled();
    if writer.override_interactivity(widget, next.interactivity()) {
        *mode = next;
        info!(
            "widgets: secondary button is now {:?}",
            next.interactivity()
        );
    } else {
        warn!("widgets: failed to update secondary button interactivity");
    }
}
