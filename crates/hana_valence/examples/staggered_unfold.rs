//! Five hinged panels form a staged accordion from a visible fixed mount.

use bevy::anti_alias::taa::TemporalAntiAliasing;
use bevy::camera::primitives::Aabb;
use bevy::color::Srgba;
use bevy::color::palettes::css::CORAL;
use bevy::color::palettes::css::GOLD;
use bevy::color::palettes::css::MEDIUM_PURPLE;
use bevy::color::palettes::css::SEA_GREEN;
use bevy::color::palettes::css::SILVER;
use bevy::color::palettes::css::SKY_BLUE;
use bevy::color::palettes::css::TURQUOISE;
use bevy::math::Dir3;
use bevy::prelude::*;
use bevy_kana::ToF32;
use bevy_lagrange::OrbitCamPreset;
use fairy_dust::Anchor;
use fairy_dust::CameraHomeTarget;
use fairy_dust::DescriptionPanel;
use fairy_dust::TitleBar;
use hana_diegetic::DiegeticText;
use hana_diegetic::Sidedness;
use hana_valence::AnchorId;
use hana_valence::AnchorPose;
use hana_valence::AnchoredTo;
use hana_valence::Edge;
use hana_valence::FoldAngles;
use hana_valence::FoldMember;
use hana_valence::FoldSequence;
use hana_valence::FoldStage;
use hana_valence::Hinge;
use hana_valence::HingePivot;
use hana_valence::ResolvedAnchorGeometry;

#[path = "../fixtures.rs"]
#[allow(
    dead_code,
    reason = "shared geometry fixtures; this example uses a subset"
)]
mod fixtures;

use fixtures::QUAD_LEFT_EDGE;

// app
const EXAMPLE_TITLE: &str = "Staggered Unfold";

// folding
const FOLD_SECONDS: f32 = 0.8;
const FULL_FOLD_ANGLE: f32 = core::f32::consts::PI;
const HALF_FOLD_ANGLE: f32 = core::f32::consts::FRAC_PI_2;
// Panel 1 stops parallel to the fixed mount. The remaining signs alternate for
// the accordion motion; ±PI close to the same panel plane, forming a stack.
const PANEL_FOLD_ANGLES: [f32; PANEL_COUNT] = [
    HALF_FOLD_ANGLE,
    -FULL_FOLD_ANGLE,
    FULL_FOLD_ANGLE,
    -FULL_FOLD_ANGLE,
    FULL_FOLD_ANGLE,
];
const PANEL_ATTACHMENT_OFFSETS: [Vec3; PANEL_COUNT] = [
    Vec3::new(0.0, 0.0, (PANEL_THICKNESS - MOUNT_DEPTH) / 2.0),
    Vec3::ZERO,
    Vec3::ZERO,
    Vec3::ZERO,
    Vec3::ZERO,
];

// camera home
const HOME_MARGIN: f32 = 0.43;
const HOME_OFFSET_PX: Vec2 = Vec2::new(-55.0, 55.0);
const HOME_PITCH: f32 = 0.36;
const HOME_TARGET_NAME: &str = "Fully unfolded chain bounds";
const HOME_TARGET_POSITION: Vec3 =
    Vec3::new(-(MOUNT_WIDTH + BASE_WIDTH) / 4.0, MOUNT_HEIGHT / 2.0, 0.0);
const HOME_TARGET_SIZE: Vec3 = Vec3::new(
    PANEL_SPAN + f32::midpoint(MOUNT_WIDTH, BASE_WIDTH),
    MOUNT_HEIGHT,
    BASE_DEPTH,
);
const HOME_YAW: f32 = 0.37;

// description panel
const DESCRIPTION_LINES: [&str; 9] = [
    "Gold fixed root owns the chain transform and no fold stage.",
    "Panels 1–5 use consecutive authored FoldMember stages.",
    "Segmented knuckles mark invariant HingePivot axes.",
    "Half-turn stages stack panel faces after panel 1 clears the root.",
    "Space / Shift+Space steps one stage forward / backward.",
    "At a terminal, P selects the other endpoint.",
    "Idle in the interior: P follows the latest step direction.",
    "During a step: P continues that direction to the terminal.",
    "During Play: P reverses immediately.",
];
const DESCRIPTION_TITLE: &str = "Authored Accordion";

// labels
const FIRST_PANEL_NUMBER: usize = 1;
const FIXED_ROOT_LABEL: &str = "FIXED ROOT";
const FIXED_ROOT_LABEL_OFFSET: Vec3 =
    Vec3::new(0.0, MOUNT_HEIGHT / 2.0 + 0.25, MOUNT_DEPTH / 2.0 + 0.03);
const FIXED_ROOT_LABEL_SIZE: f32 = 0.18;
const LABEL_COLOR: Color = Color::BLACK;
const LABEL_SIZE: f32 = 0.32;
const LABEL_Z_OFFSET: f32 = PANEL_THICKNESS / 2.0 + 0.006;

// mount
const BASE_DEPTH: f32 = 0.8;
const BASE_HEIGHT: f32 = 0.14;
const BASE_WIDTH: f32 = 0.9;
const MOUNT_DEPTH: f32 = 0.42;
const MOUNT_HEIGHT: f32 = 1.8;
const MOUNT_POSITION: Vec3 = Vec3::new(
    -PANEL_SPAN / 2.0 - MOUNT_WIDTH / 2.0,
    MOUNT_HEIGHT / 2.0,
    0.0,
);
const METAL_ROUGHNESS: f32 = 0.24;
const MOUNT_WIDTH: f32 = 0.28;

// panels
const HINGE_HEIGHT: f32 = PANEL_HEIGHT + 0.12;
const HINGE_KNUCKLE_COUNT: usize = 3;
const HINGE_KNUCKLE_GAP: f32 = 0.08;
const HINGE_RADIUS: f32 = 0.065;
const PANEL_COLORS: [Srgba; PANEL_COUNT] = [CORAL, SKY_BLUE, SEA_GREEN, MEDIUM_PURPLE, TURQUOISE];
const PANEL_COUNT: usize = 5;
const PANEL_HEIGHT: f32 = 1.2;
const PANEL_ROUGHNESS: f32 = 0.42;
const PANEL_SOURCE_ANCHOR: AnchorId = AnchorId::EdgeMid(3);
const PANEL_SPAN: f32 = 7.25;
const PANEL_TARGET_ANCHOR: AnchorId = AnchorId::EdgeMid(1);
const PANEL_THICKNESS: f32 = 0.08;
const PANEL_WIDTH: f32 = 1.45;

// scene
const GROUND_SIZE: f32 = 11.0;

#[derive(Clone, Copy)]
enum Face {
    Front,
    Back,
}

struct PanelAssets {
    knuckle_material: Handle<StandardMaterial>,
    knuckle_mesh:     Handle<Mesh>,
    panel_mesh:       Handle<Mesh>,
}

#[derive(Clone, Copy)]
struct PanelJoint {
    attachment_offset: Vec3,
    edge:              Edge,
    folded_angle:      f32,
    pivot_offset:      Vec3,
    source_anchor:     AnchorId,
    target_anchor:     AnchorId,
}

impl PanelJoint {
    fn new(folded_angle: f32, attachment_offset: Vec3) -> Self {
        Self {
            attachment_offset,
            edge: QUAD_LEFT_EDGE,
            folded_angle,
            pivot_offset: Vec3::Z * (-folded_angle.signum() * PANEL_THICKNESS / 2.0),
            source_anchor: PANEL_SOURCE_ANCHOR,
            target_anchor: PANEL_TARGET_ANCHOR,
        }
    }

    fn knuckle_line(self, geometry: &ResolvedAnchorGeometry) -> Option<KnuckleLine> {
        let direction = self.edge.axis(geometry).ok()?;
        let source_point = geometry.points.get(&self.source_anchor)?;
        Some(KnuckleLine {
            center: source_point.position + source_point.rotation() * self.pivot_offset,
            direction,
        })
    }
}

#[derive(Clone, Copy)]
struct KnuckleLine {
    center:    Vec3,
    direction: Dir3,
}

fn main() {
    // `hana_diegetic::DiegeticUiPlugin` is registered automatically by
    // `fairy_dust::sprinkle_example`.
    let app = fairy_dust::sprinkle_example()
        .with_brp_extras()
        .with_save_window_position()
        .with_studio_lighting()
        .with_ground_plane()
        .size(GROUND_SIZE)
        .with_orbit_cam_preset_bundle(
            |_| {},
            OrbitCamPreset::blender_like(),
            (Msaa::Off, TemporalAntiAliasing::default()),
        )
        .with_stable_transparency()
        .with_camera_home()
        .pitch(HOME_PITCH)
        .yaw(HOME_YAW)
        .margin(HOME_MARGIN)
        .offset_px(HOME_OFFSET_PX)
        .with_camera_control_panel()
        .with_fold_controls()
        .with_title_bar(
            TitleBar::new()
                .with_title(EXAMPLE_TITLE)
                .with_anchor(Anchor::TopLeft),
        )
        .with_description_panel(description_panel());
    app.add_systems(Startup, setup).run();
}

fn description_panel() -> DescriptionPanel {
    DescriptionPanel::new(DESCRIPTION_TITLE)
        .with_fit_width()
        .lines(DESCRIPTION_LINES)
}

fn setup(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let panel_assets = PanelAssets {
        knuckle_material: materials.add(metal_material(SILVER)),
        knuckle_mesh:     meshes.add(Cylinder::new(HINGE_RADIUS, hinge_knuckle_height())),
        panel_mesh:       meshes.add(Cuboid::new(PANEL_WIDTH, PANEL_HEIGHT, PANEL_THICKNESS)),
    };
    let panel_materials = PANEL_COLORS.map(|color| materials.add(panel_material(color)));

    spawn_home_target(&mut commands);
    let sequence = commands.spawn(FoldSequence::new(FOLD_SECONDS)).id();
    let mut parent = spawn_fixed_mount(&mut commands, &mut meshes, &mut materials);
    for (stage, ((folded_angle, attachment_offset), material)) in PANEL_FOLD_ANGLES
        .into_iter()
        .zip(PANEL_ATTACHMENT_OFFSETS)
        .zip(panel_materials)
        .enumerate()
    {
        parent = spawn_hinged_panel(
            &mut commands,
            &panel_assets,
            material,
            parent,
            sequence,
            FoldStage(stage),
            PanelJoint::new(folded_angle, attachment_offset),
            stage + FIRST_PANEL_NUMBER,
        );
    }
}

// Every `AnchoredTo` chain ends at an entity whose `Transform` is authored
// instead of resolved. The gold mount is that reference: it exposes anchor
// geometry, but carries neither `AnchoredTo`, `Hinge`, nor `FoldMember`.
fn spawn_fixed_mount(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
) -> Entity {
    let material = materials.add(metal_material(GOLD));
    let mount = commands
        .spawn((
            Mesh3d(meshes.add(Cuboid::new(MOUNT_WIDTH, MOUNT_HEIGHT, MOUNT_DEPTH))),
            MeshMaterial3d(material.clone()),
            fixtures::quad_geometry(MOUNT_WIDTH, MOUNT_HEIGHT),
            Transform::from_translation(MOUNT_POSITION),
            GlobalTransform::from_translation(MOUNT_POSITION),
        ))
        .id();
    commands.entity(mount).with_children(|root| {
        root.spawn((
            Mesh3d(meshes.add(Cuboid::new(BASE_WIDTH, BASE_HEIGHT, BASE_DEPTH))),
            MeshMaterial3d(material),
            Transform::from_xyz(0.0, BASE_HEIGHT / 2.0 - MOUNT_POSITION.y, 0.0),
        ));
        root.spawn(fixed_root_label());
    });
    mount
}

fn spawn_hinged_panel(
    commands: &mut Commands,
    panel_assets: &PanelAssets,
    material: Handle<StandardMaterial>,
    parent: Entity,
    sequence: Entity,
    stage: FoldStage,
    joint: PanelJoint,
    number: usize,
) -> Entity {
    let geometry = fixtures::quad_geometry(PANEL_WIDTH, PANEL_HEIGHT);
    let knuckle_line = joint.knuckle_line(&geometry);
    let entity = commands
        .spawn((
            Mesh3d(panel_assets.panel_mesh.clone()),
            MeshMaterial3d(material),
            geometry,
            Transform::default(),
            GlobalTransform::default(),
            AnchoredTo::new(parent, joint.source_anchor, joint.target_anchor)
                .with_offset(joint.attachment_offset),
            AnchorPose::default(),
            Hinge {
                edge:  joint.edge,
                angle: 0.0,
            },
            HingePivot {
                offset:          joint.pivot_offset,
                reference_angle: 0.0,
            },
            FoldMember::new(sequence, stage),
            FoldAngles {
                unfolded: 0.0,
                folded:   joint.folded_angle,
            },
        ))
        .id();
    spawn_panel_details(commands, panel_assets, entity, number, knuckle_line);
    entity
}

// The render AABBs are not available when the startup home view is authored.
// This proxy covers the unfolded panels, fixed mount, and base so startup and
// `H Home` frame the complete physical attachment.
fn spawn_home_target(commands: &mut Commands) {
    let half_size = HOME_TARGET_SIZE / 2.0;
    commands.spawn((
        Name::new(HOME_TARGET_NAME),
        CameraHomeTarget,
        Aabb::from_min_max(
            HOME_TARGET_POSITION - half_size,
            HOME_TARGET_POSITION + half_size,
        ),
        Transform::default(),
    ));
}

fn spawn_panel_details(
    commands: &mut Commands,
    panel_assets: &PanelAssets,
    panel: Entity,
    number: usize,
    knuckle_line: Option<KnuckleLine>,
) {
    commands.entity(panel).with_children(|visual| {
        if let Some(knuckle_line) = knuckle_line {
            let rotation = Quat::from_rotation_arc(Vec3::Y, *knuckle_line.direction);
            let stride = hinge_knuckle_height() + HINGE_KNUCKLE_GAP;
            let center_index = (HINGE_KNUCKLE_COUNT - 1).to_f32() / 2.0;
            for index in 0..HINGE_KNUCKLE_COUNT {
                let offset = (index.to_f32() - center_index) * stride;
                visual.spawn((
                    Mesh3d(panel_assets.knuckle_mesh.clone()),
                    MeshMaterial3d(panel_assets.knuckle_material.clone()),
                    Transform::from_translation(
                        knuckle_line.center + *knuckle_line.direction * offset,
                    )
                    .with_rotation(rotation),
                ));
            }
        }
        visual.spawn(panel_label(number, Face::Front));
        visual.spawn(panel_label(number, Face::Back));
    });
}

fn hinge_knuckle_height() -> f32 {
    let gaps = (HINGE_KNUCKLE_COUNT - 1).to_f32();
    (HINGE_HEIGHT - gaps * HINGE_KNUCKLE_GAP) / HINGE_KNUCKLE_COUNT.to_f32()
}

fn panel_label(number: usize, face: Face) -> impl Bundle {
    let (offset, facing) = match face {
        Face::Front => (LABEL_Z_OFFSET, Quat::IDENTITY),
        Face::Back => (
            -LABEL_Z_OFFSET,
            Quat::from_rotation_y(core::f32::consts::PI),
        ),
    };
    DiegeticText::world(number.to_string())
        .size(LABEL_SIZE)
        .color(LABEL_COLOR)
        .sidedness(Sidedness::FrontOnly)
        .transform(Transform::from_xyz(0.0, 0.0, offset).with_rotation(facing))
        .build()
}

fn fixed_root_label() -> impl Bundle {
    DiegeticText::world(FIXED_ROOT_LABEL)
        .size(FIXED_ROOT_LABEL_SIZE)
        .color(Color::from(GOLD))
        .sidedness(Sidedness::FrontOnly)
        .transform(Transform::from_translation(FIXED_ROOT_LABEL_OFFSET))
        .build()
}

fn panel_material(color: Srgba) -> StandardMaterial {
    StandardMaterial {
        base_color: Color::from(color),
        perceptual_roughness: PANEL_ROUGHNESS,
        ..default()
    }
}

fn metal_material(color: Srgba) -> StandardMaterial {
    StandardMaterial {
        base_color: Color::from(color),
        metallic: 1.0,
        perceptual_roughness: METAL_ROUGHNESS,
        ..default()
    }
}
