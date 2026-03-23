//! @generated `bevy_example_template`
//! Glyph render mode × shadow mode matrix.
//!
//! Displays a large "A" for every combination of [`GlyphRenderMode`] and
//! [`GlyphShadowMode`], arranged in a grid in front of an opaque panel.
//! A directional light shines through the text toward the panel so each
//! shadow mode is visible. Labels below each glyph describe the active
//! combination.
//!
//! Click the ground to zoom-to-fit the full scene; click any text or the
//! panel to zoom-to-fit that entity.

use std::time::Duration;

use bevy::picking::mesh_picking::MeshPickingPlugin;
use bevy::prelude::*;
use bevy_brp_extras::BrpExtrasPlugin;
use bevy_brp_extras::PortDisplay;
use bevy_diegetic::DiegeticUiPlugin;
use bevy_diegetic::GlyphRenderMode;
use bevy_diegetic::GlyphShadowMode;
use bevy_diegetic::TextStyle;
use bevy_diegetic::WorldText;
use bevy_panorbit_camera::PanOrbitCamera;
use bevy_panorbit_camera::PanOrbitCameraPlugin;
use bevy_panorbit_camera::TrackpadBehavior;
use bevy_panorbit_camera_ext::PanOrbitCameraExtPlugin;
use bevy_panorbit_camera_ext::ZoomToFit;
use bevy_window_manager::WindowManagerPlugin;

const ZOOM_MARGIN_ENTITY: f32 = 0.15;
const ZOOM_MARGIN_SCENE: f32 = 0.08;
const ZOOM_DURATION_MS: u64 = 1000;

/// Font size for the large glyph.
const GLYPH_SIZE: f32 = 64.0;

/// Font size for row and column headers.
const HEADER_SIZE: f32 = 12.0;

/// Horizontal spacing between columns.
const COL_SPACING: f32 = 1.8;

/// Vertical spacing between rows.
const ROW_SPACING: f32 = 1.8;

/// Distance between the text plane and the shadow-receiver panel.
const PANEL_OFFSET: f32 = 2.0;

#[derive(Resource)]
struct SceneBounds(Entity);

fn main() {
    App::new()
        .add_plugins((
            DefaultPlugins,
            DiegeticUiPlugin,
            PanOrbitCameraPlugin,
            PanOrbitCameraExtPlugin,
            BrpExtrasPlugin::default().port_in_title(PortDisplay::NonDefault),
            WindowManagerPlugin,
            MeshPickingPlugin,
        ))
        .add_systems(Startup, setup)
        .run();
}

const fn render_mode_label(mode: GlyphRenderMode) -> &'static str {
    match mode {
        GlyphRenderMode::Invisible => "Invisible",
        GlyphRenderMode::Text => "Text",
        GlyphRenderMode::PunchOut => "PunchOut",
        GlyphRenderMode::SolidQuad => "SolidQuad",
    }
}

const fn shadow_mode_label(mode: GlyphShadowMode) -> &'static str {
    match mode {
        GlyphShadowMode::None => "None",
        GlyphShadowMode::SolidQuad => "SolidQuad",
        GlyphShadowMode::Text => "Text",
        GlyphShadowMode::PunchOut => "PunchOut",
    }
}

#[allow(clippy::too_many_lines)]
fn setup(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let render_modes = [
        GlyphRenderMode::Text,
        GlyphRenderMode::PunchOut,
        GlyphRenderMode::SolidQuad,
        GlyphRenderMode::Invisible,
    ];
    let shadow_modes = [
        GlyphShadowMode::None,
        GlyphShadowMode::SolidQuad,
        GlyphShadowMode::Text,
        GlyphShadowMode::PunchOut,
    ];

    let cols = shadow_modes.len();
    let rows = render_modes.len();

    #[allow(clippy::cast_precision_loss)]
    let grid_width = (cols - 1) as f32 * COL_SPACING;
    #[allow(clippy::cast_precision_loss)]
    let grid_height = (rows - 1) as f32 * ROW_SPACING;
    let grid_center_y = grid_height * 0.5 + 1.5;

    // Ground plane — root of the scene hierarchy. All UI is parented here
    // so moving the plane moves everything.
    let ground = commands
        .spawn((
            Mesh3d(meshes.add(Plane3d::default().mesh().size(20.0, 20.0))),
            MeshMaterial3d(materials.add(StandardMaterial {
                base_color: Color::srgba(0.3, 0.5, 0.3, 0.8),
                alpha_mode: AlphaMode::Blend,
                double_sided: true,
                cull_mode: None,
                ..default()
            })),
        ))
        .observe(on_ground_clicked)
        .id();

    commands.insert_resource(SceneBounds(ground));

    // Shadow-receiver panel — opaque backdrop behind the text grid.
    let panel_w = grid_width + 3.0;
    let panel_h = grid_height + 3.0;
    let backdrop = commands
        .spawn((
            Mesh3d(meshes.add(Rectangle::new(panel_w, panel_h))),
            MeshMaterial3d(materials.add(StandardMaterial {
                base_color: Color::srgb(0.85, 0.85, 0.85),
                double_sided: true,
                cull_mode: None,
                ..default()
            })),
            Transform::from_xyz(0.0, grid_center_y, -PANEL_OFFSET),
        ))
        .observe(on_entity_clicked)
        .id();
    commands.entity(ground).add_child(backdrop);

    let label_color = Color::srgb(0.1, 0.15, 0.6);
    let label_shadow_color = Color::WHITE;
    let label_shadow_offset = Vec3::new(0.0015, -0.0015, -0.001);

    // Row headers (render mode labels on the left).
    #[allow(clippy::cast_precision_loss)]
    for (row, &render_mode) in render_modes.iter().enumerate() {
        let y = ((rows - 1 - row) as f32).mul_add(ROW_SPACING, grid_center_y) - grid_height * 0.5;
        spawn_label(
            &mut commands,
            ground,
            render_mode_label(render_mode),
            HEADER_SIZE,
            label_color,
            label_shadow_color,
            label_shadow_offset,
            Vec3::new((-grid_width).mul_add(0.5, -1.5), y, 0.0),
        );
    }

    // Column headers (shadow mode labels on top).
    #[allow(clippy::cast_precision_loss)]
    for (col, &shadow_mode) in shadow_modes.iter().enumerate() {
        let x = (col as f32).mul_add(COL_SPACING, -(grid_width * 0.5));
        let y = grid_center_y + grid_height.mul_add(0.5, 0.8);
        spawn_label(
            &mut commands,
            ground,
            shadow_mode_label(shadow_mode),
            HEADER_SIZE,
            label_color,
            label_shadow_color,
            label_shadow_offset,
            Vec3::new(x, y, 0.0),
        );
    }

    // Spawn one "A" per (render_mode, shadow_mode) combination.
    #[allow(clippy::cast_precision_loss)]
    for (row, &render_mode) in render_modes.iter().enumerate() {
        for (col, &shadow_mode) in shadow_modes.iter().enumerate() {
            let x = (col as f32).mul_add(COL_SPACING, -(grid_width * 0.5));
            let y =
                ((rows - 1 - row) as f32).mul_add(ROW_SPACING, grid_center_y) - grid_height * 0.5;

            let glyph = commands
                .spawn((
                    WorldText::new("A"),
                    TextStyle::new()
                        .with_size(GLYPH_SIZE)
                        .with_color(Color::srgb(0.2, 0.4, 0.9))
                        .with_render_mode(render_mode)
                        .with_shadow_mode(shadow_mode),
                    Transform::from_xyz(x, y, 0.0),
                ))
                .observe(on_entity_clicked)
                .id();
            commands.entity(ground).add_child(glyph);
        }
    }

    // Directional light — shines from the camera side through text toward the panel.
    commands.spawn((
        DirectionalLight {
            shadows_enabled: true,
            illuminance: 5000.0,
            ..default()
        },
        Transform::from_xyz(0.0, 5.0, 8.0).looking_at(Vec3::new(0.0, grid_center_y, 0.0), Vec3::Y),
    ));

    // Camera + ambient light.
    commands.spawn((
        AmbientLight {
            color:                      Color::WHITE,
            brightness:                 200.0,
            affects_lightmapped_meshes: false,
        },
        PanOrbitCamera {
            focus: Vec3::new(-0.766, 3.732, -1.97),
            radius: Some(12.7),
            yaw: Some(0.033),
            pitch: Some(-0.152),
            button_orbit: MouseButton::Middle,
            button_pan: MouseButton::Middle,
            modifier_pan: Some(KeyCode::ShiftLeft),
            trackpad_behavior: TrackpadBehavior::BlenderLike {
                modifier_pan:  Some(KeyCode::ShiftLeft),
                modifier_zoom: Some(KeyCode::ControlLeft),
            },

            trackpad_pinch_to_zoom_enabled: true,
            ..default()
        },
    ));

    // HUD label.
    commands.spawn((
        Text::new("rows: GlyphRenderMode, columns: GlyphShadowMode"),
        TextFont {
            font_size: 14.0,
            ..default()
        },
        TextColor(Color::srgba(1.0, 1.0, 1.0, 0.6)),
        Node {
            position_type: PositionType::Absolute,
            bottom: Val::Px(12.0),
            left: Val::Px(12.0),
            ..default()
        },
    ));
}

fn on_entity_clicked(
    mut click: On<Pointer<Click>>,
    children: Query<&Children>,
    meshes: Query<(), With<Mesh3d>>,
    mut commands: Commands,
) {
    click.propagate(false);
    let camera = click.hit.camera;
    let target = children
        .get(click.entity)
        .ok()
        .and_then(|kids| kids.iter().find(|&kid| meshes.contains(kid)))
        .unwrap_or(click.entity);
    commands.trigger(
        ZoomToFit::new(camera, target)
            .margin(ZOOM_MARGIN_ENTITY)
            .duration(Duration::from_millis(ZOOM_DURATION_MS)),
    );
}

fn on_ground_clicked(click: On<Pointer<Click>>, mut commands: Commands, scene: Res<SceneBounds>) {
    let camera = click.hit.camera;
    commands.trigger(
        ZoomToFit::new(camera, scene.0)
            .margin(ZOOM_MARGIN_SCENE)
            .duration(Duration::from_millis(ZOOM_DURATION_MS)),
    );
}

/// Spawns a label with a white drop shadow, click-to-zoom, parented to `parent`.
#[allow(clippy::too_many_arguments)]
fn spawn_label(
    commands: &mut Commands,
    parent: Entity,
    text: &str,
    size: f32,
    color: Color,
    shadow_color: Color,
    shadow_offset: Vec3,
    pos: Vec3,
) {
    let style = TextStyle::new()
        .with_size(size)
        .with_shadow_mode(GlyphShadowMode::None);
    let shadow = commands
        .spawn((
            WorldText::new(text),
            style.clone().with_color(shadow_color),
            Transform::from_translation(pos + shadow_offset),
        ))
        .id();
    let label = commands
        .spawn((
            WorldText::new(text),
            style.with_color(color),
            Transform::from_translation(pos),
        ))
        .observe(on_entity_clicked)
        .id();
    commands.entity(parent).add_children(&[shadow, label]);
}
