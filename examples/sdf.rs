#![allow(
    clippy::expect_used,
    reason = "demo code; panic on invalid setup is acceptable"
)]
#![allow(
    clippy::cast_precision_loss,
    clippy::suboptimal_flops,
    reason = "demo SDF math; grid indices and shader coordinates stay within f32 precision"
)]

//! SDF primitive lab.
//!
//! Static comparison scene for experimenting with line rendering in world
//! space. The first pass focuses on vertical line segments at multiple
//! widths:
//! - raw line `shape_kind = 4`
//! - one-sided border-strip workaround
//! - border-reference panel edges

use std::time::Duration;

use bevy::asset::Asset;
use bevy::color::Alpha;
use bevy::color::Color;
use bevy::color::LinearRgba;
use bevy::light::NotShadowCaster;
use bevy::math::Vec2;
use bevy::math::Vec4;
use bevy::pbr::ExtendedMaterial;
use bevy::pbr::MaterialExtension;
use bevy::pbr::MaterialPlugin;
use bevy::pbr::StandardMaterial;
use bevy::prelude::*;
use bevy::reflect::TypePath;
use bevy::render::render_resource::AsBindGroup;
use bevy::render::render_resource::ShaderType;
use bevy::shader::ShaderRef;
use bevy_brp_extras::BrpExtrasPlugin;
use bevy_brp_extras::PortDisplay;
use bevy_diegetic::AlignY;
use bevy_diegetic::Anchor;
use bevy_diegetic::Border;
use bevy_diegetic::CornerRadius;
use bevy_diegetic::DiegeticPanel;
use bevy_diegetic::DiegeticUiPlugin;
use bevy_diegetic::Direction;
use bevy_diegetic::El;
use bevy_diegetic::FontId;
use bevy_diegetic::LayoutBuilder;
use bevy_diegetic::LayoutTextStyle;
use bevy_diegetic::Padding;
use bevy_diegetic::Pt;
use bevy_diegetic::Px;
use bevy_diegetic::Sizing;
use bevy_diegetic::WorldText;
use bevy_diegetic::WorldTextStyle;
use bevy_lagrange::AnimateToFit;
use bevy_lagrange::InputControl;
use bevy_lagrange::LagrangePlugin;
use bevy_lagrange::OrbitCam;
use bevy_lagrange::TrackpadBehavior;
use bevy_lagrange::TrackpadInput;
use bevy_window_manager::WindowManagerPlugin;

const GROUND_SIZE: f32 = 8.0;
const DISPLAY_Z: f32 = 2.2;

const KEY_LIGHT_LUX: f32 = 15_000.0;
const KEY_LIGHT_POS: Vec3 = Vec3::new(0.0, 2.5, 6.0);
const REFLECTION_LIGHT_LEVEL: f32 = 150_000.0;
const REFLECTION_LIGHT_POS: Vec3 = Vec3::new(0.7, 1.9, 6.2);
const REFLECTION_TARGET: Vec3 = Vec3::new(0.15, 0.0, 1.7);

// ── Grid layout ─────────────────────────────────────────────────────
// Three SDF rows × six columns. Every row sits above the ground plane
// (y = 0) and places its visible stroke at the column X. Rows are
// spaced just enough to keep their strokes visually separate without
// wasting vertical space.
const LINE_LENGTH: f32 = 0.25;
const ROW_Y: [f32; 3] = [0.85, 0.58, 0.31];
const START_X: f32 = -0.75;
const X_STEP: f32 = 0.3;
const ROW_LABEL_X: f32 = -1.5;
const TITLE_Y: f32 = 1.0;
const WIDTH_LABEL_Y: f32 = 0.12;
const ROW_Z: f32 = DISPLAY_Z;

const LINE_COLOR: Color = Color::srgb(0.92, 0.92, 0.92);
const REF_COLOR: Color = Color::srgba(1.0, 1.0, 0.6, 0.7);

// 1 point = 1/72 inch.
const METERS_PER_PT: f32 = 0.025_4 / 72.0;

// Stroke widths in typographic points. Labels display in pt.
const WIDTHS_PT: [f32; 6] = [3.0, 4.0, 6.0, 8.0, 14.0, 28.0];

// Mesh breathing room: the mesh half-thickness is
// `MESH_THICKNESS_MULTIPLIER * stroke/2`. Matching this ratio across
// every row keeps `fwidth` on the thickness axis equally stable, so
// the AA band has room to fall off before the quad edge regardless of
// stroke width.
const MESH_THICKNESS_MULTIPLIER: f32 = 21.0;

// Border-edge row draws the stroke as the border of a larger
// transparent rectangle. The hidden region is
// `(MESH_THICKNESS_MULTIPLIER - 1) * stroke/2` on one side of the
// visible stroke, producing the same mesh-to-stroke ratio as the
// other rows.
const BORDER_EDGE_HIDDEN_HALVES: f32 = MESH_THICKNESS_MULTIPLIER - 1.0;

// Absolute mesh padding beyond the SDF outer boundary. The AA band for
// the outer edge falls off over ~one pixel; this gives it unconditional
// breathing room regardless of stroke width. Only the border-edge row
// needs it explicitly — rows 1/2 already have ~10× stroke of implicit
// padding from the thickness multiplier, which is plenty.
const SDF_AA_PADDING: f32 = 0.002;

const HOME_YAW: f32 = 0.0;
const HOME_PITCH: f32 = 0.18;
const HOME_MARGIN: f32 = 0.08;
const HOME_DELAY_SECS: f32 = 0.1;
const HOME_DURATION_MS: u64 = 900;

const CONTROLS_WIDTH: Px = Px(170.0);
const CONTROLS_HEIGHT: Px = Px(54.0);
const CONTROLS_TITLE_SIZE: Pt = Pt(14.0);
const CONTROLS_HINT_SIZE: Pt = Pt(11.0);
const CONTROLS_RADIUS: Px = Px(14.0);
const CONTROLS_BACKGROUND: Color = Color::srgba(0.02, 0.03, 0.07, 0.82);
const CONTROLS_FRAME: Color = Color::srgba(0.01, 0.01, 0.03, 0.95);
const CONTROLS_ACCENT: Color = Color::srgba(0.15, 0.7, 0.9, 0.5);
const CONTROLS_BORDER: Color = Color::srgba(0.1, 0.4, 0.6, 0.3);
const CONTROLS_TITLE_COLOR: Color = Color::srgb(0.9, 0.95, 1.0);
const CONTROLS_HINT_COLOR: Color = Color::srgba(0.7, 0.75, 0.85, 0.92);
const CAMERA_HEADER_COLOR: Color = Color::srgb(1.0, 0.82, 0.52);
const CONTROLS_DIVIDER_COLOR: Color = Color::srgba(0.15, 0.4, 0.6, 0.25);
const CAM_HELP_WIDTH: Px = Px(280.0);
const CAM_HELP_HEIGHT: Px = Px(160.0);
const CAM_HELP_LABEL_SIZE: Pt = Pt(11.0);
const CAM_HELP_HEADER_SIZE: Pt = Pt(13.0);
const CAM_HELP_TITLE_SIZE: Pt = Pt(16.0);
const CAM_HELP_RADIUS: Px = Px(15.0);
const CAM_HELP_FRAME_PAD: Px = Px(2.0);
const CAM_HELP_BORDER: Px = Px(2.0);
const CAM_HELP_INSET: Px = Px(CAM_HELP_FRAME_PAD.0 + CAM_HELP_BORDER.0);
const CAM_HELP_INNER_RADIUS: Px = Px(CAM_HELP_RADIUS.0 - CAM_HELP_INSET.0);

#[derive(Component)]
struct LabRoot;

#[derive(Component)]
struct ControlsPanel;

#[derive(Component)]
struct CameraHelpPanel;

#[derive(Resource)]
struct SceneBounds(Entity);

#[derive(Resource)]
struct HomeOnStart(Timer);

#[derive(Asset, AsBindGroup, Clone, Debug, TypePath)]
struct ExampleSdfExtension {
    #[uniform(100)]
    uniforms: ExampleSdfUniform,
}

#[derive(Clone, Debug, ShaderType)]
struct ExampleSdfUniform {
    half_size:        Vec2,
    mesh_half_size:   Vec2,
    corner_radii:     Vec4,
    border_widths:    Vec4,
    border_color:     Vec4,
    shape_kind:       u32,
    shape_params:     Vec4,
    fill_alpha:       f32,
    clip_rect:        Vec4,
    oit_depth_offset: f32,
}

type ExampleSdfMaterial = ExtendedMaterial<StandardMaterial, ExampleSdfExtension>;

impl MaterialExtension for ExampleSdfExtension {
    fn fragment_shader() -> ShaderRef { "shaders/sdf_panel.wgsl".into() }

    fn prepass_fragment_shader() -> ShaderRef { "shaders/sdf_panel.wgsl".into() }
}

fn main() {
    App::new()
        .add_plugins((
            DefaultPlugins,
            DiegeticUiPlugin,
            LagrangePlugin,
            BrpExtrasPlugin::default().port_in_title(PortDisplay::NonDefault),
            WindowManagerPlugin,
            MaterialPlugin::<ExampleSdfMaterial>::default(),
        ))
        .insert_resource(HomeOnStart(Timer::from_seconds(
            HOME_DELAY_SECS,
            TimerMode::Once,
        )))
        .add_systems(Startup, setup)
        .add_systems(Update, (fit_camera_on_start, home_camera))
        .run();
}

fn setup(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut std_materials: ResMut<Assets<StandardMaterial>>,
    mut sdf_materials: ResMut<Assets<ExampleSdfMaterial>>,
) {
    let root = commands
        .spawn((LabRoot, Transform::IDENTITY, Visibility::Visible))
        .id();
    let bounds = spawn_scene_bounds(&mut commands, root, &mut meshes, &mut std_materials);
    commands.insert_resource(SceneBounds(bounds));

    spawn_ground(&mut commands, &mut meshes, &mut std_materials);
    spawn_lights(&mut commands);
    spawn_camera(&mut commands);
    spawn_controls_panel(&mut commands);
    spawn_camera_help_panel(&mut commands);
    spawn_labels(&mut commands, root);
    spawn_line_rows(&mut commands, root, &mut meshes, &mut sdf_materials);
}

fn spawn_scene_bounds(
    commands: &mut Commands,
    parent: Entity,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
) -> Entity {
    // Bounds frame the visible lab content: from just left of the row
    // labels to just right of the last column, and from the ground up
    // to just above the title.
    let left = ROW_LABEL_X - 0.6;
    let right = START_X + (WIDTHS_PT.len() as f32 - 1.0) * X_STEP + 0.3;
    let top = TITLE_Y + 0.15;
    let bottom = 0.0;
    let width = right - left;
    let height = top - bottom;
    let center = Vec3::new(
        f32::midpoint(left, right),
        f32::midpoint(top, bottom),
        ROW_Z,
    );

    commands
        .spawn((
            Name::new("SdfSceneBounds"),
            NotShadowCaster,
            Mesh3d(meshes.add(Rectangle::new(width, height))),
            MeshMaterial3d(materials.add(StandardMaterial {
                base_color: Color::NONE,
                alpha_mode: AlphaMode::Blend,
                unlit: true,
                ..default()
            })),
            Transform::from_translation(center),
            Visibility::Inherited,
            ChildOf(parent),
        ))
        .id()
}

fn spawn_ground(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
) {
    commands.spawn((
        Mesh3d(meshes.add(Plane3d::default().mesh().size(GROUND_SIZE, GROUND_SIZE))),
        MeshMaterial3d(materials.add(StandardMaterial {
            base_color: Color::srgb(0.08, 0.08, 0.08),
            perceptual_roughness: 0.15,
            metallic: 0.0,
            double_sided: true,
            cull_mode: None,
            ..default()
        })),
        Transform::from_xyz(0.0, 0.0, DISPLAY_Z),
    ));
}

fn spawn_lights(commands: &mut Commands) {
    commands.spawn((
        DirectionalLight {
            illuminance: KEY_LIGHT_LUX,
            shadows_enabled: true,
            ..default()
        },
        Transform::from_translation(KEY_LIGHT_POS)
            .looking_at(Vec3::new(0.0, 1.0, DISPLAY_Z), Vec3::Y),
    ));

    commands.spawn((
        SpotLight {
            intensity: REFLECTION_LIGHT_LEVEL,
            shadows_enabled: false,
            inner_angle: 0.22,
            outer_angle: 0.38,
            range: 12.0,
            ..default()
        },
        Transform::from_translation(REFLECTION_LIGHT_POS).looking_at(REFLECTION_TARGET, Vec3::Y),
    ));
}

fn spawn_camera(commands: &mut Commands) {
    commands.spawn((OrbitCam {
        focus: Vec3::new(0.0, 1.0, DISPLAY_Z),
        radius: Some(4.2),
        yaw: Some(0.0),
        pitch: Some(0.18),
        button_orbit: MouseButton::Middle,
        button_pan: MouseButton::Middle,
        modifier_pan: Some(KeyCode::ShiftLeft),
        input_control: Some(InputControl {
            trackpad: Some(TrackpadInput {
                behavior:    TrackpadBehavior::BlenderLike {
                    modifier_pan:  Some(KeyCode::ShiftLeft),
                    modifier_zoom: Some(KeyCode::ControlLeft),
                },
                sensitivity: 0.5,
            }),
            ..default()
        }),
        ..default()
    },));
}

fn spawn_controls_panel(commands: &mut Commands) {
    let unlit_material = bevy_diegetic::default_panel_material();
    let unlit = StandardMaterial {
        unlit: true,
        ..unlit_material
    };

    commands.spawn((
        ControlsPanel,
        DiegeticPanel::screen()
            .size(
                Sizing::fixed(CONTROLS_WIDTH),
                Sizing::fixed(CONTROLS_HEIGHT),
            )
            .anchor(Anchor::TopLeft)
            .material(unlit.clone())
            .text_material(unlit)
            .with_tree(build_controls_panel())
            .build()
            .expect("valid controls HUD dimensions"),
        Transform::default(),
    ));
}

fn build_controls_panel() -> bevy_diegetic::LayoutTree {
    let title = LayoutTextStyle::new(CONTROLS_TITLE_SIZE)
        .with_font(FontId::MONOSPACE.0)
        .with_color(CONTROLS_TITLE_COLOR);
    let hint = LayoutTextStyle::new(CONTROLS_HINT_SIZE)
        .with_font(FontId::MONOSPACE.0)
        .with_color(CONTROLS_HINT_COLOR);

    let mut builder = LayoutBuilder::new(CONTROLS_WIDTH, CONTROLS_HEIGHT);
    builder.with(
        El::new()
            .width(Sizing::GROW)
            .height(Sizing::GROW)
            .padding(Padding::all(Px(2.0)))
            .corner_radius(CornerRadius::new(
                CONTROLS_RADIUS,
                CONTROLS_RADIUS,
                CONTROLS_RADIUS,
                CONTROLS_RADIUS,
            ))
            .background(CONTROLS_FRAME)
            .border(Border::all(Px(2.0), CONTROLS_ACCENT)),
        |b| {
            b.with(
                El::new()
                    .width(Sizing::GROW)
                    .height(Sizing::GROW)
                    .direction(Direction::LeftToRight)
                    .padding(Padding::all(Px(10.0)))
                    .child_gap(Px(10.0))
                    .child_align_y(AlignY::Center)
                    .corner_radius(CornerRadius::new(Px(11.0), Px(11.0), Px(11.0), Px(11.0)))
                    .background(CONTROLS_BACKGROUND)
                    .border(Border::all(Px(1.0), CONTROLS_BORDER)),
                |b| {
                    b.text("CONTROLS", title);
                    hud_separator(b);
                    b.text("H Home", hint);
                },
            );
        },
    );
    builder.build()
}

fn spawn_camera_help_panel(commands: &mut Commands) {
    let unlit_material = bevy_diegetic::default_panel_material();
    let unlit = StandardMaterial {
        unlit: true,
        ..unlit_material
    };

    commands.spawn((
        CameraHelpPanel,
        DiegeticPanel::screen()
            .size(
                Sizing::fixed(CAM_HELP_WIDTH),
                Sizing::fixed(CAM_HELP_HEIGHT),
            )
            .anchor(Anchor::BottomRight)
            .material(unlit.clone())
            .text_material(unlit)
            .layout(build_camera_help)
            .build()
            .expect("valid camera help HUD dimensions"),
        Transform::default(),
    ));
}

fn hud_separator(b: &mut LayoutBuilder) {
    b.with(
        El::new()
            .width(Sizing::fixed(Px(1.0)))
            .height(Sizing::GROW)
            .background(CONTROLS_DIVIDER_COLOR),
        |_| {},
    );
}

fn build_camera_help(b: &mut LayoutBuilder) {
    let title = LayoutTextStyle::new(CAM_HELP_TITLE_SIZE)
        .with_font(FontId::MONOSPACE.0)
        .with_color(CONTROLS_TITLE_COLOR);
    let header = LayoutTextStyle::new(CAM_HELP_HEADER_SIZE)
        .with_font(FontId::MONOSPACE.0)
        .with_color(CAMERA_HEADER_COLOR);
    let label = LayoutTextStyle::new(CAM_HELP_LABEL_SIZE)
        .with_font(FontId::MONOSPACE.0)
        .with_color(CONTROLS_HINT_COLOR);

    b.with(
        El::new()
            .width(Sizing::GROW)
            .height(Sizing::GROW)
            .padding(Padding::all(CAM_HELP_FRAME_PAD))
            .corner_radius(CornerRadius::new(
                CAM_HELP_RADIUS,
                CAM_HELP_RADIUS,
                CAM_HELP_RADIUS,
                CAM_HELP_RADIUS,
            ))
            .background(CONTROLS_FRAME)
            .border(Border::all(CAM_HELP_BORDER, CONTROLS_ACCENT)),
        |b| {
            b.with(
                El::new()
                    .width(Sizing::GROW)
                    .height(Sizing::GROW)
                    .direction(Direction::TopToBottom)
                    .padding(Padding::all(Px(10.0)))
                    .child_gap(Px(6.0))
                    .corner_radius(CornerRadius::new(
                        CAM_HELP_INNER_RADIUS,
                        CAM_HELP_INNER_RADIUS,
                        CAM_HELP_INNER_RADIUS,
                        CAM_HELP_INNER_RADIUS,
                    ))
                    .background(CONTROLS_BACKGROUND)
                    .border(Border::all(Px(1.0), CONTROLS_BORDER)),
                |b| {
                    b.text("CAMERA", title);

                    b.with(
                        El::new()
                            .width(Sizing::GROW)
                            .height(Sizing::GROW)
                            .direction(Direction::LeftToRight)
                            .child_gap(Px(12.0)),
                        |b| {
                            b.with(
                                El::new()
                                    .width(Sizing::GROW)
                                    .direction(Direction::TopToBottom)
                                    .child_gap(Px(4.0)),
                                |b| {
                                    b.text("Mouse", header.clone());
                                    b.text("MMB drag → Orbit", label.clone());
                                    b.text("Shift+MMB → Pan", label.clone());
                                    b.text("Scroll → Zoom", label.clone());
                                },
                            );

                            b.with(
                                El::new()
                                    .width(Sizing::fixed(Px(1.0)))
                                    .height(Sizing::GROW)
                                    .background(CONTROLS_DIVIDER_COLOR),
                                |_| {},
                            );

                            b.with(
                                El::new()
                                    .width(Sizing::GROW)
                                    .direction(Direction::TopToBottom)
                                    .child_gap(Px(4.0)),
                                |b| {
                                    b.text("Trackpad", header.clone());
                                    b.text("Scroll → Orbit", label.clone());
                                    b.text("Shift+Scroll → Pan", label.clone());
                                    b.text("Ctrl+Scroll → Zoom", label.clone());
                                    b.text("Pinch → Zoom", label.clone());
                                },
                            );
                        },
                    );
                },
            );
        },
    );
}

fn spawn_labels(commands: &mut Commands, parent: Entity) {
    commands.entity(parent).with_children(|parent| {
        // Title sits above all three rows.
        parent.spawn((
            WorldText::new("SDF Line Lab"),
            WorldTextStyle::new(0.14).with_color(Color::srgb(0.8, 0.9, 1.0)),
            Transform::from_xyz(0.0, TITLE_Y, DISPLAY_Z),
        ));

        // Row labels: one per row, placed inline at ROW_LABEL_X so they
        // sit at the same height as the strokes they describe.
        for (text, y) in [
            ("raw line shape_kind=4", ROW_Y[0]),
            ("stretched rect shape_kind=0", ROW_Y[1]),
            ("border edge", ROW_Y[2]),
        ] {
            parent.spawn((
                WorldText::new(text),
                WorldTextStyle::new(0.06)
                    .with_color(Color::srgb(0.9, 0.9, 0.95))
                    .with_shadow_mode(bevy_diegetic::GlyphShadowMode::Text),
                Transform::from_xyz(ROW_LABEL_X, y, ROW_Z),
            ));
        }

        // Column width labels: one per column, placed below the bottom
        // row but above the ground plane.
        for (index, pt) in WIDTHS_PT.iter().enumerate() {
            let x = START_X + index as f32 * X_STEP;
            parent.spawn((
                WorldText::new(format!("{pt}pt")),
                WorldTextStyle::new(0.07).with_color(Color::srgb(0.7, 0.75, 0.85)),
                Transform::from_xyz(x, WIDTH_LABEL_Y, ROW_Z),
            ));
        }
    });
}

fn spawn_line_rows(
    commands: &mut Commands,
    root: Entity,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<ExampleSdfMaterial>,
) {
    for (index, pt) in WIDTHS_PT.iter().enumerate() {
        let x = START_X + index as f32 * X_STEP;
        let width = pt * METERS_PER_PT;
        spawn_raw_line(commands, root, meshes, materials, x, ROW_Y[0], width);
        spawn_stretched_rect(commands, root, meshes, materials, x, ROW_Y[1], width);
        spawn_border_edge(commands, root, meshes, materials, x, ROW_Y[2], width);
    }
}

fn fit_camera_on_start(
    time: Res<Time>,
    mut home_on_start: ResMut<HomeOnStart>,
    cameras: Query<Entity, With<OrbitCam>>,
    scene: Res<SceneBounds>,
    mut initialized: Local<bool>,
    mut commands: Commands,
) {
    if *initialized || !home_on_start.0.tick(time.delta()).just_finished() {
        return;
    }
    *initialized = true;
    trigger_home_camera(&cameras, scene.0, &mut commands);
}

fn home_camera(
    keyboard: Res<ButtonInput<KeyCode>>,
    cameras: Query<Entity, With<OrbitCam>>,
    scene: Res<SceneBounds>,
    mut commands: Commands,
) {
    if !keyboard.just_pressed(KeyCode::KeyH) {
        return;
    }
    trigger_home_camera(&cameras, scene.0, &mut commands);
}

fn trigger_home_camera(
    cameras: &Query<Entity, With<OrbitCam>>,
    target: Entity,
    commands: &mut Commands,
) {
    for camera in cameras.iter() {
        commands.trigger(
            AnimateToFit::new(camera, target)
                .yaw(HOME_YAW)
                .pitch(HOME_PITCH)
                .margin(HOME_MARGIN)
                .duration(Duration::from_millis(HOME_DURATION_MS))
                .easing(bevy::math::curve::easing::EaseFunction::CubicOut),
        );
    }
}

// ── Line-spawn helpers ──────────────────────────────────────────────
//
// Every approach builds the mesh in "line-local" space where X is the
// stroke length and Y is its thickness, then rotates -90° about Z so
// the stroke stands vertically in world space. This is required for
// `shape_kind = 4` (sd_line_segment), which expects a horizontal
// segment; the other rows follow the same convention for consistency.

fn rotate_vertical() -> Quat { Quat::from_rotation_z(-std::f32::consts::FRAC_PI_2) }

/// Row 0 — `sd_line_segment` (`shape_kind` = 4). The path the doc
/// flags as producing artifacts.
fn spawn_raw_line(
    commands: &mut Commands,
    parent: Entity,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<ExampleSdfMaterial>,
    x: f32,
    y: f32,
    width: f32,
) {
    spawn_line_with_shape(
        commands, parent, meshes, materials, x, y, width, 4, LINE_COLOR,
    );
}

/// Row 1 — `sd_rounded_box` with radii = 0 and `half_size.y` equal
/// to the stroke half-thickness. A filled thin rectangle treated as
/// a line. Isolates whether the raw-line artifacts come from the
/// line SDF itself or from the generic shape/alpha path.
fn spawn_stretched_rect(
    commands: &mut Commands,
    parent: Entity,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<ExampleSdfMaterial>,
    x: f32,
    y: f32,
    width: f32,
) {
    spawn_line_with_shape(
        commands, parent, meshes, materials, x, y, width, 0, LINE_COLOR,
    );
}

/// Shared filled-shape spawner for rows 0 and 1. Same mesh, same
/// transform, only the `shape_kind` differs.
fn spawn_line_with_shape(
    commands: &mut Commands,
    parent: Entity,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<ExampleSdfMaterial>,
    x: f32,
    y: f32,
    width: f32,
    shape_kind: u32,
    color: Color,
) {
    let half_length = LINE_LENGTH * 0.5;
    let half_thickness = width * 0.5;
    // Pad the mesh to MESH_THICKNESS_MULTIPLIER × the SDF half-thickness
    // so `fwidth(local.y)` has breathing room. Pad the length direction
    // by the same absolute amount (which is generous since the line is
    // long already).
    let mesh_half_w = half_length + half_thickness * (MESH_THICKNESS_MULTIPLIER - 1.0);
    let mesh_half_h = half_thickness * MESH_THICKNESS_MULTIPLIER;
    let material = materials.add(example_sdf_material(
        color,
        half_length,
        half_thickness,
        mesh_half_w,
        mesh_half_h,
        [0.0; 4],
        [0.0; 4],
        None,
        shape_kind,
    ));
    commands.entity(parent).with_child((
        Mesh3d(meshes.add(Rectangle::new(mesh_half_w * 2.0, mesh_half_h * 2.0))),
        MeshMaterial3d(material),
        Transform::from_xyz(x, y, ROW_Z).with_rotation(rotate_vertical()),
    ));
}

/// Row 2 — the known-good reference. Draws the stroke as the border
/// of a much larger transparent rectangle, exercising the same path
/// that panel borders use.
fn spawn_border_edge(
    commands: &mut Commands,
    parent: Entity,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<ExampleSdfMaterial>,
    x: f32,
    y: f32,
    width: f32,
) {
    let half_length = LINE_LENGTH * 0.5;
    let stroke_half = width * 0.5;
    let hidden_extent = stroke_half * BORDER_EDGE_HIDDEN_HALVES;
    let half_thickness = hidden_extent + stroke_half;
    // The visible stroke sits right on the outer SDF boundary on the
    // thickness axis, so padding is required for the AA band to fall
    // off before the mesh edge. Scale with stroke width so thick
    // strokes (e.g. 28pt) still get enough pixels of breathing room at
    // oblique viewing angles.
    let outer_thickness_pad = SDF_AA_PADDING + stroke_half;
    let mesh_half_w = half_length + stroke_half * (MESH_THICKNESS_MULTIPLIER - 1.0);
    let mesh_half_h = half_thickness + outer_thickness_pad;

    // Border widths are [top, right, bottom, left] — draw the bottom
    // edge. After the -90° Z rotation that edge becomes the left side
    // in world space, so shift the quad right by (half_thickness -
    // width/2) to center the visible stroke on column X.
    let material = materials.add(example_sdf_material(
        Color::NONE,
        half_length,
        half_thickness,
        mesh_half_w,
        mesh_half_h,
        [0.0; 4],
        [0.0, 0.0, width, 0.0],
        Some(REF_COLOR),
        0,
    ));
    let visible_edge_offset = Vec3::X * (half_thickness - width * 0.5);
    commands.entity(parent).with_child((
        Mesh3d(meshes.add(Rectangle::new(mesh_half_w * 2.0, mesh_half_h * 2.0))),
        MeshMaterial3d(material),
        Transform::from_translation(Vec3::new(x, y, ROW_Z) + visible_edge_offset)
            .with_rotation(rotate_vertical()),
    ));
}

fn example_sdf_material(
    base_color: Color,
    half_width: f32,
    half_height: f32,
    mesh_half_width: f32,
    mesh_half_height: f32,
    corner_radii: [f32; 4],
    border_widths: [f32; 4],
    border_color: Option<Color>,
    shape_kind: u32,
) -> ExampleSdfMaterial {
    // Single-sided, back-face culled. Double-sided + cull_mode=None
    // caused front and back fragments to both emit partial-alpha pixels
    // for the same SDF, producing interference banding that depended on
    // viewing angle.
    let mut base = StandardMaterial {
        base_color,
        unlit: true,
        alpha_mode: AlphaMode::Blend,
        ..default()
    };
    let fill_alpha = base.base_color.alpha();
    let border_linear: Vec4 = border_color.map_or(Vec4::ZERO, |c| {
        let l: LinearRgba = c.into();
        Vec4::new(l.red, l.green, l.blue, l.alpha)
    });

    ExtendedMaterial {
        base:      {
            base.alpha_mode = AlphaMode::Blend;
            base
        },
        extension: ExampleSdfExtension {
            uniforms: ExampleSdfUniform {
                half_size: Vec2::new(half_width, half_height),
                mesh_half_size: Vec2::new(mesh_half_width, mesh_half_height),
                corner_radii: Vec4::from_array(corner_radii),
                border_widths: Vec4::from_array(border_widths),
                border_color: border_linear,
                shape_kind,
                shape_params: Vec4::ZERO,
                fill_alpha,
                clip_rect: Vec4::new(
                    -mesh_half_width,
                    -mesh_half_height,
                    mesh_half_width,
                    mesh_half_height,
                ),
                oit_depth_offset: 0.0,
            },
        },
    }
}
