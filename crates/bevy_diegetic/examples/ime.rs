//! IME editing example for panel fields and app-owned popups.

use bevy::picking::mesh_picking::MeshPickingPlugin;
use bevy::prelude::*;
use bevy::window::PrimaryWindow;
use bevy_diegetic::AlignX;
use bevy_diegetic::AlignY;
use bevy_diegetic::Anchor;
use bevy_diegetic::Border;
use bevy_diegetic::CornerRadius;
use bevy_diegetic::DiegeticPanel;
use bevy_diegetic::DiegeticPanelCommands;
use bevy_diegetic::Direction;
use bevy_diegetic::El;
use bevy_diegetic::ImeAcceptCommit;
use bevy_diegetic::ImeAppInputDisposition;
use bevy_diegetic::ImeAppInputDispositionHook;
use bevy_diegetic::ImeAppOwnedFieldSpec;
use bevy_diegetic::ImeAppliedResult;
use bevy_diegetic::ImeBuiltInFieldKind;
use bevy_diegetic::ImeBuiltInFieldSpec;
use bevy_diegetic::ImeCommitAuthority;
use bevy_diegetic::ImeCommitRequested;
use bevy_diegetic::ImeEditableFieldSpec;
use bevy_diegetic::ImeOpenSession;
use bevy_diegetic::ImeRejectCommit;
use bevy_diegetic::ImeRejection;
use bevy_diegetic::ImeSessionAnchor;
use bevy_diegetic::ImeTarget;
use bevy_diegetic::ImeValueRevision;
use bevy_diegetic::LayoutBuilder;
use bevy_diegetic::LayoutTextStyle;
use bevy_diegetic::Mm;
use bevy_diegetic::Padding;
use bevy_diegetic::Px;
use bevy_diegetic::Sizing;
use fairy_dust::DescriptionPanel;
use fairy_dust::Face;
use fairy_dust::OrbitCam;
use fairy_dust::TitleBar;
use fairy_dust::sprinkle_example;

// app panel
const APP_ANCHOR_MAX: Vec2 = Vec2::new(552.0, 166.0);
const APP_ANCHOR_MIN: Vec2 = Vec2::new(214.0, 134.0);
const APP_PANEL_HEIGHT: f32 = 150.0;
const APP_PANEL_WIDTH: f32 = 560.0;
const APP_SCREEN_X: f32 = 24.0;
const APP_SCREEN_Y: f32 = 74.0;

// scene
const CAMERA_FOCUS: Vec3 = Vec3::new(0.0, 0.72, 0.0);
const CAMERA_PITCH: f32 = -0.10;
const CAMERA_RADIUS: f32 = 2.7;
const CAMERA_YAW: f32 = 0.0;
const CUBE_SIZE: f32 = 0.55;
const GROUND_SIZE: f32 = 4.2;
const HOME_MARGIN: f32 = 0.18;
const HOME_SCALE: Vec3 = Vec3::new(2.6, 1.6, 1.8);
const HOME_TRANSLATION: Vec3 = Vec3::new(0.0, 0.72, 0.0);
const PANEL_TRANSLATION: Vec3 = Vec3::new(0.0, 1.08, 0.0);
const SCENE_TARGET: Vec3 = Vec3::new(0.0, 0.7, 0.0);

// style
const ACCENT: Color = Color::srgb(0.36, 0.82, 0.94);
const APP_PANEL_BACKGROUND: Color = Color::srgba(0.035, 0.045, 0.052, 0.94);
const BODY: Color = Color::srgb(0.76, 0.80, 0.84);
const BORDER: Color = Color::srgba(0.30, 0.73, 0.88, 0.58);
const CONSOLE_PANEL_BACKGROUND: Color = Color::srgba(0.055, 0.070, 0.080, 0.96);
const CUBE_COLOR: Color = Color::srgb(0.18, 0.24, 0.28);
const FIELD_BACKGROUND: Color = Color::srgba(0.08, 0.13, 0.14, 0.98);
const GROUND_COLOR: Color = Color::srgba(0.11, 0.13, 0.14, 1.0);
const MUTED: Color = Color::srgb(0.50, 0.56, 0.60);
const TITLE: Color = Color::srgb(0.90, 0.96, 1.0);
const VALUE: Color = Color::srgb(0.55, 0.92, 0.66);
const WARNING: Color = Color::srgb(1.0, 0.48, 0.36);

const BODY_SIZE: f32 = 13.0;
const LABEL_SIZE: f32 = 12.0;
const TITLE_SIZE: f32 = 17.0;
const VALUE_SIZE: f32 = 16.0;

#[derive(Component)]
struct AppSearchOwner;

#[derive(Component)]
struct AppSearchPanel;

#[derive(Resource, Clone, Debug)]
struct AppSearchState {
    text:     String,
    revision: u64,
    error:    Option<String>,
}

impl Default for AppSearchState {
    fn default() -> Self {
        Self {
            text:     "dock manifest".to_owned(),
            revision: 1,
            error:    None,
        }
    }
}

fn main() {
    sprinkle_example()
        .with_title_bar(title_bar())
        .with_description_panel(description_panel())
        .with_studio_lighting()
        .aim_at(SCENE_TARGET)
        .with_ground_plane()
        .size(GROUND_SIZE)
        .color(GROUND_COLOR)
        .with_cube()
        .size(CUBE_SIZE)
        .color(CUBE_COLOR)
        .transform(Transform::from_xyz(0.0, CUBE_SIZE * 0.5, 0.0))
        .face_text(Face::Front, "IME", 0.11, ACCENT)
        .with_camera_home(home_transform())
        .margin(HOME_MARGIN)
        .with_orbit_cam_configured(configure_camera)
        .with_restore_camera_on_restart()
        .with_stable_transparency()
        .with_camera_control_panel()
        .with_save_window_position()
        .with_brp_extras()
        .add_plugins(MeshPickingPlugin)
        .init_resource::<AppSearchState>()
        .add_systems(Startup, setup_ime_panels)
        .add_systems(Update, (open_app_search, refresh_app_search_panel))
        .add_observer(apply_app_search_commit)
        .run();
}

fn title_bar() -> TitleBar {
    TitleBar::new().with_title("IME editing").controls([
        "Double-click Gain",
        "/ Search",
        "Enter Apply",
        "Esc Cancel",
    ])
}

fn description_panel() -> DescriptionPanel {
    DescriptionPanel::new("IME surfaces")
        .line("World field: double-click the gain value.")
        .line("App-owned entry: press / and edit search text.")
        .line("Invalid numeric input stays open for correction.")
}

fn home_transform() -> Transform {
    Transform::from_translation(HOME_TRANSLATION).with_scale(HOME_SCALE)
}

fn configure_camera(orbit_cam: &mut OrbitCam) {
    orbit_cam.focus = CAMERA_FOCUS;
    orbit_cam.radius = Some(CAMERA_RADIUS);
    orbit_cam.yaw = Some(CAMERA_YAW);
    orbit_cam.pitch = Some(CAMERA_PITCH);
}

fn setup_ime_panels(
    mut commands: Commands,
    mut hook: ResMut<ImeAppInputDispositionHook>,
    search: Res<AppSearchState>,
) {
    hook.set(|context| {
        if context.keys.just_pressed(KeyCode::Tab) {
            ImeAppInputDisposition::Surface
        } else {
            ImeAppInputDisposition::Edit
        }
    });
    spawn_world_panel(&mut commands);
    spawn_app_search_panel(&mut commands, &search);
}

fn spawn_world_panel(commands: &mut Commands) {
    let panel = DiegeticPanel::world()
        .size(Mm(132.0), Mm(58.0))
        .anchor(Anchor::Center)
        .layout(world_panel_tree)
        .build();
    let Ok(panel) = panel else {
        error!("failed to build IME world panel");
        return;
    };
    commands.spawn((panel, Transform::from_translation(PANEL_TRANSLATION)));
}

fn world_panel_tree(builder: &mut LayoutBuilder) {
    builder.with(
        El::new()
            .width(Sizing::GROW)
            .height(Sizing::GROW)
            .padding(Padding::all(6.0))
            .direction(Direction::TopToBottom)
            .child_gap(5.0)
            .background(CONSOLE_PANEL_BACKGROUND)
            .corner_radius(CornerRadius::all(4.0))
            .border(Border::all(1.0, BORDER)),
        |builder| {
            builder.text(
                "Signal console",
                LayoutTextStyle::new(TITLE_SIZE).with_color(TITLE),
            );
            metric_row(builder, "Channel", "A-17", BODY);
            editable_gain_row(builder);
            builder.text(
                "Range 0.0-10.0",
                LayoutTextStyle::new(LABEL_SIZE).with_color(MUTED),
            );
        },
    );
}

fn metric_row(builder: &mut LayoutBuilder, label: &str, value: &str, value_color: Color) {
    builder.with(
        El::new()
            .width(Sizing::GROW)
            .height(Sizing::FIT)
            .direction(Direction::LeftToRight)
            .child_alignment(AlignX::Left, AlignY::Center)
            .child_gap(8.0),
        |builder| {
            builder.with(El::new().width(Sizing::fixed(48.0)), |builder| {
                builder.text(label, LayoutTextStyle::new(BODY_SIZE).with_color(MUTED));
            });
            builder.text(
                value,
                LayoutTextStyle::new(BODY_SIZE).with_color(value_color),
            );
        },
    );
}

fn editable_gain_row(builder: &mut LayoutBuilder) {
    builder.with(
        El::new()
            .width(Sizing::GROW)
            .height(Sizing::FIT)
            .direction(Direction::LeftToRight)
            .child_alignment(AlignX::Left, AlignY::Center)
            .child_gap(8.0),
        |builder| {
            builder.with(El::new().width(Sizing::fixed(48.0)), |builder| {
                builder.text("Gain", LayoutTextStyle::new(BODY_SIZE).with_color(MUTED));
            });
            builder.with(editable_value_box("gain"), |builder| {
                builder.text("1.25", LayoutTextStyle::new(VALUE_SIZE).with_color(VALUE));
            });
        },
    );
}

fn editable_value_box(field_id: &'static str) -> El {
    El::new()
        .width(Sizing::fixed(48.0))
        .height(Sizing::FIT)
        .padding(Padding::xy(5.0, 2.0))
        .background(FIELD_BACKGROUND)
        .corner_radius(CornerRadius::all(2.0))
        .border(Border::all(1.0, ACCENT))
        .editable_field(
            field_id,
            ImeEditableFieldSpec::BuiltIn(ImeBuiltInFieldSpec::new(ImeBuiltInFieldKind::Float {
                min: Some(0.0),
                max: Some(10.0),
            })),
        )
}

fn spawn_app_search_panel(commands: &mut Commands, search: &AppSearchState) {
    let owner = commands.spawn(AppSearchOwner).id();
    let panel = DiegeticPanel::screen()
        .size(
            Sizing::fixed(Px(APP_PANEL_WIDTH)),
            Sizing::fixed(Px(APP_PANEL_HEIGHT)),
        )
        .anchor(Anchor::TopLeft)
        .screen_position(APP_SCREEN_X, APP_SCREEN_Y)
        .layout(|builder| app_search_tree(builder, search))
        .build();
    let Ok(panel) = panel else {
        error!("failed to build IME app-owned panel");
        return;
    };
    commands.spawn((AppSearchPanel, panel, Transform::default(), ChildOf(owner)));
}

fn app_search_tree(builder: &mut LayoutBuilder, search: &AppSearchState) {
    builder.with(
        El::new()
            .width(Sizing::GROW)
            .height(Sizing::GROW)
            .padding(Padding::all(8.0))
            .direction(Direction::TopToBottom)
            .child_gap(7.0)
            .background(APP_PANEL_BACKGROUND)
            .corner_radius(CornerRadius::all(5.0))
            .border(Border::all(1.0, BORDER)),
        |builder| {
            builder.text(
                "App-owned search",
                LayoutTextStyle::new(TITLE_SIZE).with_color(TITLE),
            );
            app_search_row(builder, "Shortcut", "Press /", ACCENT);
            app_search_row(builder, "Query", search.text.as_str(), VALUE);
            if let Some(error) = &search.error {
                builder.text(
                    error.as_str(),
                    LayoutTextStyle::new(BODY_SIZE).with_color(WARNING),
                );
            }
        },
    );
}

fn app_search_row(builder: &mut LayoutBuilder, label: &str, value: &str, value_color: Color) {
    builder.with(
        El::new()
            .width(Sizing::GROW)
            .height(Sizing::FIT)
            .direction(Direction::LeftToRight)
            .child_alignment(AlignX::Left, AlignY::Center)
            .child_gap(10.0),
        |builder| {
            builder.with(El::new().width(Sizing::fixed(82.0)), |builder| {
                builder.text(label, LayoutTextStyle::new(BODY_SIZE).with_color(MUTED));
            });
            builder.with(
                El::new()
                    .width(Sizing::fixed(338.0))
                    .height(Sizing::FIT)
                    .padding(Padding::xy(7.0, 2.0))
                    .background(FIELD_BACKGROUND)
                    .corner_radius(CornerRadius::all(2.0)),
                |builder| {
                    builder.text(
                        value,
                        LayoutTextStyle::new(VALUE_SIZE).with_color(value_color),
                    );
                },
            );
        },
    );
}

fn open_app_search(
    keys: Res<ButtonInput<KeyCode>>,
    windows: Query<Entity, With<PrimaryWindow>>,
    owners: Query<Entity, With<AppSearchOwner>>,
    search: Res<AppSearchState>,
    mut commands: Commands,
) {
    if !keys.just_pressed(KeyCode::Slash) {
        return;
    }
    let Ok(window) = windows.single() else { return };
    let Ok(owner) = owners.single() else { return };

    commands.trigger(ImeOpenSession {
        target: ImeTarget::AppOwned {
            owner,
            field_id: "search".into(),
        },
        window,
        initial_text: search.text.clone(),
        field_spec: ImeEditableFieldSpec::AppOwned(ImeAppOwnedFieldSpec::new("search")),
        anchor: Some(ImeSessionAnchor::screen_rect(Rect::from_corners(
            APP_ANCHOR_MIN,
            APP_ANCHOR_MAX,
        ))),
    });
}

fn apply_app_search_commit(
    event: On<ImeCommitRequested>,
    authority: Res<ImeCommitAuthority>,
    mut search: ResMut<AppSearchState>,
    mut commands: Commands,
) {
    let event = event.event();
    if !authority.is_current(event.session_id, event.attempt_id) {
        return;
    }
    if !matches!(event.target, ImeTarget::AppOwned { .. }) {
        return;
    }
    if event.text.trim().is_empty() {
        commands.trigger(ImeRejectCommit {
            session_id: event.session_id,
            attempt_id: event.attempt_id,
            reason:     ImeRejection::AppOwned("search cannot be empty".to_owned()),
        });
        return;
    }

    event.text.trim().clone_into(&mut search.text);
    search.revision = search.revision.wrapping_add(1);
    search.error = None;
    commands.trigger(ImeAcceptCommit {
        session_id: event.session_id,
        attempt_id: event.attempt_id,
        result:     ImeAppliedResult::AppOwned {
            display_text:   Some(search.text.clone()),
            value_revision: Some(ImeValueRevision::new(search.revision)),
        },
    });
}

fn refresh_app_search_panel(
    search: Res<AppSearchState>,
    panels: Query<Entity, With<AppSearchPanel>>,
    mut commands: Commands,
) {
    if !search.is_changed() {
        return;
    }
    let Ok(panel) = panels.single() else { return };
    let mut builder = LayoutBuilder::new(Px(APP_PANEL_WIDTH), Px(APP_PANEL_HEIGHT));
    app_search_tree(&mut builder, &search);
    commands.set_tree(panel, builder.build());
}
