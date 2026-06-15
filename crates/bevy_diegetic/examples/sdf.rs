//! SDF primitive lab.
//!
//! Static comparison scene for line rendering in world space. Three rows show
//! the same vertical stroke at six widths through different SDF paths:
//! - raw line `sdf_kind = 4`
//! - stretched rounded-rect `sdf_kind = 0`
//! - border edge (the panel-border path)
//!
//! All three render through the crate's embedded `sdf_panel.wgsl`. Orbit to a
//! grazing angle to compare line quality across the three approaches.

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
use bevy_diegetic::DiegeticText;
use bevy_kana::ToF32;
use bevy_lagrange::OrbitCamPreset;
use fairy_dust::Anchor;
use fairy_dust::CameraHomeTarget;
use fairy_dust::TitleBar;

const DISPLAY_Z: f32 = 2.2;

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
const SDF_PANEL_SHADER_PATH: &str = "embedded://bevy_diegetic/shaders/sdf_panel.wgsl";

// Camera home pose.
const HOME_YAW: f32 = 0.0;
const HOME_PITCH: f32 = 0.18;
const HOME_MARGIN: f32 = 0.08;

#[derive(Component)]
struct LabRoot;

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
    sdf_kind:         u32,
    sdf_params:       Vec4,
    fill_alpha:       f32,
    clip_rect:        Vec4,
    oit_depth_offset: f32,
}

type ExampleSdfMaterial = ExtendedMaterial<StandardMaterial, ExampleSdfExtension>;

impl MaterialExtension for ExampleSdfExtension {
    fn fragment_shader() -> ShaderRef { SDF_PANEL_SHADER_PATH.into() }

    fn prepass_fragment_shader() -> ShaderRef { SDF_PANEL_SHADER_PATH.into() }
}

fn main() {
    // `bevy_diegetic::DiegeticUiPlugin` is registered automatically by
    // `fairy_dust::sprinkle_example`.
    fairy_dust::sprinkle_example()
        .with_brp_extras()
        .with_save_window_position()
        .add_plugins(MaterialPlugin::<ExampleSdfMaterial>::default())
        .with_studio_lighting()
        .with_ground_plane()
        .with_orbit_cam_preset(|_| {}, OrbitCamPreset::BlenderLike)
        .with_camera_home()
        .yaw(HOME_YAW)
        .pitch(HOME_PITCH)
        .margin(HOME_MARGIN)
        .with_title_bar(
            TitleBar::new()
                .with_title("SDF Line Lab")
                .with_anchor(Anchor::TopLeft),
        )
        .with_camera_control_panel()
        .add_systems(Startup, spawn_lab)
        .run();
}

fn spawn_lab(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut std_materials: ResMut<Assets<StandardMaterial>>,
    mut sdf_materials: ResMut<Assets<ExampleSdfMaterial>>,
) {
    let root = commands
        .spawn((LabRoot, Transform::IDENTITY, Visibility::Visible))
        .id();
    spawn_scene_bounds(&mut commands, root, &mut meshes, &mut std_materials);
    spawn_labels(&mut commands, root);
    spawn_line_rows(&mut commands, root, &mut meshes, &mut sdf_materials);
}

/// Invisible rectangle spanning the lab content. Carries `CameraHomeTarget` so
/// `.with_camera_home()` frames the whole grid — its mesh AABB is available
/// immediately at startup, unlike the labels' text meshes.
fn spawn_scene_bounds(
    commands: &mut Commands,
    parent: Entity,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
) {
    // Bounds frame the visible lab content: from just left of the row
    // labels to just right of the last column, and from the ground up
    // to just above the title.
    let left = ROW_LABEL_X - 0.6;
    let right = X_STEP.mul_add((WIDTHS_PT.len() - 1).to_f32(), START_X) + 0.3;
    let top = TITLE_Y + 0.15;
    let bottom = 0.0;
    let width = right - left;
    let height = top - bottom;
    let center = Vec3::new(
        f32::midpoint(left, right),
        f32::midpoint(top, bottom),
        ROW_Z,
    );

    commands.spawn((
        Name::new("SdfSceneBounds"),
        CameraHomeTarget,
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
    ));
}

fn spawn_labels(commands: &mut Commands, parent: Entity) {
    commands.entity(parent).with_children(|parent| {
        // Row labels: one per row, placed inline at ROW_LABEL_X so they
        // sit at the same height as the strokes they describe.
        for (text, y) in [
            ("raw line sdf_kind=4", ROW_Y[0]),
            ("stretched rect sdf_kind=0", ROW_Y[1]),
            ("border edge", ROW_Y[2]),
        ] {
            parent.spawn(
                DiegeticText::world(text)
                    .size(0.06)
                    .color(Color::srgb(0.9, 0.9, 0.95))
                    .shadow_mode(bevy_diegetic::GlyphShadowMode::Cast)
                    .transform(Transform::from_xyz(ROW_LABEL_X, y, ROW_Z))
                    .build(),
            );
        }

        // Column width labels: one per column, placed below the bottom
        // row but above the ground plane.
        for (index, pt) in WIDTHS_PT.iter().enumerate() {
            let x = X_STEP.mul_add(index.to_f32(), START_X);
            parent.spawn(
                DiegeticText::world(format!("{pt}pt"))
                    .size(0.07)
                    .color(Color::srgb(0.7, 0.75, 0.85))
                    .transform(Transform::from_xyz(x, WIDTH_LABEL_Y, ROW_Z))
                    .build(),
            );
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
        let x = X_STEP.mul_add(index.to_f32(), START_X);
        let width = pt * METERS_PER_PT;
        spawn_raw_line(commands, root, meshes, materials, x, ROW_Y[0], width);
        spawn_stretched_rect(commands, root, meshes, materials, x, ROW_Y[1], width);
        spawn_border_edge(commands, root, meshes, materials, x, ROW_Y[2], width);
    }
}

// ── Line-spawn helpers ──────────────────────────────────────────────
//
// Every approach builds the mesh in "line-local" space where X is the
// stroke length and Y is its thickness, then rotates -90° about Z so
// the stroke stands vertically in world space. This is required for
// `sdf_kind = 4` (sd_line_segment), which expects a horizontal
// segment; the other rows follow the same convention for consistency.

fn rotate_vertical() -> Quat { Quat::from_rotation_z(-std::f32::consts::FRAC_PI_2) }

/// Row 0 — `sd_line_segment` (`sdf_kind` = 4). The path the doc
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
    spawn_line_with_sdf(
        commands, parent, meshes, materials, x, y, width, 4, LINE_COLOR,
    );
}

/// Row 1 — `sd_rounded_box` with radii = 0 and `half_size.y` equal
/// to the stroke half-thickness. A filled thin rectangle treated as
/// a line. Isolates whether the raw-line artifacts come from the
/// line SDF itself or from the generic SDF/alpha path.
fn spawn_stretched_rect(
    commands: &mut Commands,
    parent: Entity,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<ExampleSdfMaterial>,
    x: f32,
    y: f32,
    width: f32,
) {
    spawn_line_with_sdf(
        commands, parent, meshes, materials, x, y, width, 0, LINE_COLOR,
    );
}

/// Shared filled-SDF spawner for rows 0 and 1. Same mesh, same transform, only
/// the `sdf_kind` differs.
fn spawn_line_with_sdf(
    commands: &mut Commands,
    parent: Entity,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<ExampleSdfMaterial>,
    x: f32,
    y: f32,
    width: f32,
    sdf_kind: u32,
    color: Color,
) {
    let half_length = LINE_LENGTH * 0.5;
    let half_thickness = width * 0.5;
    // Pad the mesh to MESH_THICKNESS_MULTIPLIER × the SDF half-thickness
    // so `fwidth(local.y)` has breathing room. Pad the length direction
    // by the same absolute amount (which is generous since the line is
    // long already).
    let mesh_half_width = half_length + half_thickness * (MESH_THICKNESS_MULTIPLIER - 1.0);
    let mesh_half_height = half_thickness * MESH_THICKNESS_MULTIPLIER;
    let material = materials.add(example_sdf_material(
        color,
        half_length,
        half_thickness,
        mesh_half_width,
        mesh_half_height,
        [0.0; 4],
        [0.0; 4],
        None,
        sdf_kind,
    ));
    commands.entity(parent).with_child((
        Mesh3d(meshes.add(Rectangle::new(
            mesh_half_width * 2.0,
            mesh_half_height * 2.0,
        ))),
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
    let mesh_half_width = half_length + stroke_half * (MESH_THICKNESS_MULTIPLIER - 1.0);
    let mesh_half_height = half_thickness + outer_thickness_pad;

    // Border widths are [top, right, bottom, left] — draw the bottom
    // edge. After the -90° Z rotation that edge becomes the left side
    // in world space, so shift the quad right by (half_thickness -
    // width/2) to center the visible stroke on column X.
    let material = materials.add(example_sdf_material(
        Color::NONE,
        half_length,
        half_thickness,
        mesh_half_width,
        mesh_half_height,
        [0.0; 4],
        [0.0, 0.0, width, 0.0],
        Some(REF_COLOR),
        0,
    ));
    let visible_edge_offset = Vec3::X * width.mul_add(-0.5, half_thickness);
    commands.entity(parent).with_child((
        Mesh3d(meshes.add(Rectangle::new(
            mesh_half_width * 2.0,
            mesh_half_height * 2.0,
        ))),
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
    sdf_kind: u32,
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
                sdf_kind,
                sdf_params: Vec4::ZERO,
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
