//! Canonical runtime example for `hana_diegetic` widgets.
//!
//! The example keeps the complete widget interaction path runnable in one
//! Fairy Dust app.
//!
//! Current controls:
//!   D - Toggle the secondary button between enabled and disabled
//!   H - Return to the camera home pose

use bevy::picking::hover::PickingInteraction;
use bevy::prelude::*;
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
use hana_diegetic::CornerRadius;
use hana_diegetic::DiegeticPanel;
use hana_diegetic::DiegeticPanelCommands;
use hana_diegetic::El;
use hana_diegetic::FitMax;
use hana_diegetic::LayoutBuilder;
use hana_diegetic::LayoutTree;
use hana_diegetic::Padding;
use hana_diegetic::PanelAnchorOffset;
use hana_diegetic::PanelAttachment;
use hana_diegetic::PanelElementId;
use hana_diegetic::PanelEntityReader;
use hana_diegetic::PanelText;
use hana_diegetic::PanelWidget;
use hana_diegetic::PanelWidgetReader;
use hana_diegetic::PanelWidgetWriter;
use hana_diegetic::Px;
use hana_diegetic::Sizing;
use hana_diegetic::Slider;
use hana_diegetic::SliderConfigError;
use hana_diegetic::SliderDirection;
use hana_diegetic::SliderRange;
use hana_diegetic::SliderStep;
use hana_diegetic::Text;
use hana_diegetic::TextStyle;
use hana_diegetic::WidgetInteractivity;
use hana_diegetic::WidgetOf;

// widget lab
const BUTTON_BORDER: Color = Color::srgba(0.30, 0.62, 1.0, 0.82);
const BUTTON_FILL: Color = Color::srgba(0.03, 0.10, 0.24, 0.82);
const BUTTON_HEIGHT: Px = Px(42.0);
const CONTROL_BORDER_WIDTH: Px = Px(1.0);
const CONTROL_GAP: Px = Px(8.0);
const CONTROL_PADDING: Px = Px(8.0);
const CONTROL_RADIUS: Px = Px(7.0);
const CONTROL_TEXT: Color = Color::srgb(0.92, 0.96, 1.0);
const CONTROL_WIDTH: Px = Px(280.0);
const CUBE_CLEARANCE: f32 = 0.1;
const DESCRIPTION_LINES: [&str; 4] = [
    "Hover each control; interaction changes are logged in the terminal.",
    "D changes the secondary button through PanelWidgetReader and PanelWidgetWriter.",
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
const PRIMARY_BUTTON_ID: &str = "primary-button";
const SECONDARY_BUTTON_ID: &str = "secondary-button";
const SECONDARY_CONTROL: &str = "D Toggle Secondary";
const SCREEN_CONTROL_WIDTH: Px = Px(218.0);
const SCREEN_PANEL_MAX_HEIGHT: Px = Px(120.0);
const SCREEN_PANEL_MAX_WIDTH: Px = Px(250.0);
const SCREEN_READOUT_COLOR: Color = Color::srgb(1.0, 0.78, 0.32);
const SCREEN_READOUT_ID: &str = "screen-anchor-status";
const SCREEN_READOUT_TEXT: &str = "^ Attached panel";
const SCREEN_TARGET_ID: &str = "screen-target-button";
const SCREEN_TARGET_LABEL: &str = "Target widget";
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
const STATUS_ID: &str = "interaction-status";
const STATUS_IDLE: &str = "Pointer: none";
const STATUS_MAX_HEIGHT: Px = Px(40.0);
const STATUS_MAX_WIDTH: Px = Px(260.0);
const STATUS_MEASURE: &str = "Pressed: secondary-button";
const STATUS_PADDING: Px = Px(6.0);
const STATUS_RADIUS: Px = Px(7.0);
const STATUS_WORLD_HEIGHT: f32 = 0.042;

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

#[derive(Component)]
struct WidgetLabPanel;

#[derive(Component)]
struct WidgetInteractionReadout;

#[derive(Component)]
struct WidgetAnchorInstalled;

#[derive(Component)]
struct ScreenWidgetLabPanel;

#[derive(Component)]
struct ScreenWidgetInteractionReadout;

#[derive(Component)]
struct ScreenWidgetAnchorInstalled;

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
        .add_systems(PostStartup, spawn_widget_lab)
        .add_systems(
            Update,
            (
                anchor_interaction_readout,
                anchor_screen_interaction_readout,
                report_interaction_changes,
            ),
        )
        .with_shortcut(KeyCode::KeyD, toggle_secondary_button)
        .run();
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
        .material(material.clone())
        .text_material(material.clone())
        .with_tree(widget_tree(slider))
        .build();
    let readout = DiegeticPanel::world()
        .size(
            FitMax(STATUS_MAX_WIDTH.into()),
            FitMax(STATUS_MAX_HEIGHT.into()),
        )
        .world_height(STATUS_WORLD_HEIGHT)
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
            FitMax(STATUS_MAX_WIDTH.into()),
            FitMax(STATUS_MAX_HEIGHT.into()),
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
    );
    add_button(
        &mut builder,
        SECONDARY_BUTTON_ID,
        "Secondary button",
        CONTROL_WIDTH,
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
        El::new()
            .width(Sizing::FIT)
            .height(Sizing::FIT)
            .padding(Padding::all(STATUS_PADDING))
            .alignment(AlignX::Center, AlignY::Center)
            .background(STATUS_BACKGROUND)
            .border(Border::all(STATUS_BORDER_WIDTH, STATUS_BORDER))
            .corner_radius(CornerRadius::all(STATUS_RADIUS)),
    );
    builder.text(
        Text::new(
            STATUS_IDLE,
            TextStyle::new(fairy_dust::LABEL_SIZE).with_color(STATUS_COLOR),
        )
        .id(STATUS_ID)
        .measure_as(STATUS_MEASURE),
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
    transform.translation.y -= (PANEL_WORLD_HEIGHT + STATUS_WORLD_HEIGHT).mul_add(0.5, STATUS_GAP);
    transform
}

fn add_button(builder: &mut LayoutBuilder, id: &'static str, label: &'static str, width: Px) {
    builder.with(
        El::new()
            .size(width, BUTTON_HEIGHT)
            .padding(Padding::all(CONTROL_PADDING))
            .alignment(AlignX::Center, AlignY::Center)
            .background(BUTTON_FILL)
            .border(Border::all(CONTROL_BORDER_WIDTH, BUTTON_BORDER))
            .corner_radius(CornerRadius::all(CONTROL_RADIUS))
            .button(id, Button::new()),
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
            active_status = Some(format!("{:?}: {}", *interaction, widget.id()));
        }
    }

    if world_interaction_changes.were_observed()
        && !panel_text.set_text(
            *readout,
            &PanelElementId::named(STATUS_ID),
            active_status.as_deref().unwrap_or(STATUS_IDLE),
        )
    {
        warn!("widgets: interaction status has not been reified");
    }
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
