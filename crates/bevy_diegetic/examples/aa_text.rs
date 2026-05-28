//! `aa_text` compares every anti-aliasing path the text renderer can use on
//! lit text standing on a ground plane under studio lighting.
//!
//! Use `1` `2` `3` `4` for the in-shader [`TextAntiAlias`] modes, `N` `S` `F`
//! `T` for Bevy post-process AA, `O` for OIT, `A` / `B` for authored camera
//! views, and `H` to return home.
//!
//! Code layout: start with `main()`, then read the `TEXT AND AA` section for
//! the text setup and AA mode switching. The later sections only support the
//! demo: camera viewpoints, explanatory panels, and scene decoration.

use std::time::Duration;

use bevy::anti_alias::fxaa::Fxaa;
use bevy::anti_alias::smaa::Smaa;
use bevy::anti_alias::taa::TemporalAntiAliasing;
use bevy::math::curve::easing::EaseFunction;
use bevy::prelude::*;
use bevy::render::camera::MipBias;
use bevy::render::camera::TemporalJitter;
use bevy::render::view::Msaa;
use bevy_diegetic::default_panel_material;
use bevy_diegetic::AlignX;
use bevy_diegetic::AlignY;
use bevy_diegetic::Anchor;
use bevy_diegetic::Border;
use bevy_diegetic::CornerRadius;
use bevy_diegetic::DiegeticPanel;
use bevy_diegetic::DiegeticPanelCommands;
use bevy_diegetic::Direction;
use bevy_diegetic::El;
use bevy_diegetic::Fit;
use bevy_diegetic::GlyphShadowMode;
use bevy_diegetic::InvalidSize;
use bevy_diegetic::LayoutBuilder;
use bevy_diegetic::LayoutTextStyle;
use bevy_diegetic::LayoutTree;
use bevy_diegetic::Padding;
use bevy_diegetic::Px;
use bevy_diegetic::Sizing;
use bevy_diegetic::StableTransparency;
use bevy_diegetic::TextAlign;
use bevy_diegetic::TextAntiAlias;
use bevy_diegetic::Unit;
use bevy_diegetic::WorldText;
use bevy_diegetic::WorldTextStyle;
use bevy_lagrange::CameraMove;
use bevy_lagrange::OrbitCam;
use bevy_lagrange::OrbitCamInputMode;
use bevy_lagrange::OrbitCamPreset;
use bevy_lagrange::PlayAnimation;
use fairy_dust::CameraHomeTarget;
use fairy_dust::ControlActivation;
use fairy_dust::TitleBar;
use fairy_dust::DEFAULT_PANEL_BACKGROUND;
use fairy_dust::LABEL_SIZE;
use fairy_dust::TITLE_SIZE;

// =============================================================================
// CONSTANTS -- static scene data, controls, copy, and panel geometry.
// =============================================================================

// Text and AA.

const EXAMPLE_TITLE: &str = "Anti-Aliasing";
const HEADLINE_TEXT: &str = "Anti-Aliasing";
const HEADLINE_SIZE: f32 = 0.40;
const HEADLINE_Y: f32 = 0.18;
const SMALL_TEXT: &str = "the quick brown fox jumps over the lazy dog";
const SMALL_SIZE: f32 = 0.05;
const SMALL_Y: f32 = -0.06;
const DISPLAY_Z: f32 = 0.0;
const TEXT_COLOR: Color = Color::srgb(0.92, 0.92, 0.94);

/// Horizontal shift applied to the headline, small line, and cube together so
/// their combined AABB straddles `x = 0`.
const SCENE_X_OFFSET: f32 = -0.4;

/// Title-bar chip that toggles OIT (stable transparency), sitting beside
/// `H Home`.
const OIT_CONTROL: &str = "O OIT";

/// The in-shader [`TextAntiAlias`] modes, in cost order. One source of truth for
/// both the key that selects each and the chip label shown for it.
const TEXT_MODES: [(KeyCode, &str, TextAntiAlias); 4] = [
    (KeyCode::Digit1, "1 Off", TextAntiAlias::Off),
    (KeyCode::Digit2, "2 Anisotropic", TextAntiAlias::Anisotropic),
    (KeyCode::Digit3, "3 SuperSample", TextAntiAlias::Supersample),
    (KeyCode::Digit4, "4 Both", TextAntiAlias::Both),
];

/// The post-process passes, with `None` as the explicit off state. One source of
/// truth for the selecting key and the chip label.
const POST_MODES: [(KeyCode, &str, PostAa); 4] = [
    (KeyCode::KeyN, "N None", PostAa::None),
    (KeyCode::KeyS, "S SMAA", PostAa::Smaa),
    (KeyCode::KeyF, "F FXAA", PostAa::Fxaa),
    (KeyCode::KeyT, "T TAA", PostAa::Taa),
];

// Camera views.

/// Opens looking down ~14 degrees so the ground plane tilts toward the camera
/// and more of the cast shadow is visible.
const HOME_PITCH: f32 = 0.24;
const HOME_YAW: f32 = 0.0;
const HOME_FIT_MARGIN: f32 = 0.1;
const HOME_FIT_DURATION_MS: u64 = 900;
const DEMO_MOVE_DURATION_MS: u64 = 1200;

/// `A` -- Off vs Anisotropic shows edge blur versus a crisp grazing edge.
const STEEP_VIEW_ROWS: [(&str, &str); 3] = [
    ("A", "go to this view"),
    ("1", "off: edges blur"),
    ("2", "Anisotropic: crisp"),
];

/// `B` -- Anisotropic crisps the edge, then Both removes the convex-corner wing.
const WING_VIEW_ROWS: [(&str, &str); 4] = [
    ("B", "go to this view"),
    ("1", "both off: wing shows"),
    ("2", "Anisotropic: most gone"),
    ("4", "Both: fully cancelled"),
];

/// The two stacked instruction boxes, top to bottom.
const DEMO_VIEWS: [DemoView; 2] = [
    DemoView {
        key:    KeyCode::KeyA,
        title:  "STEEP GRAZING ANGLE",
        focus:  Vec3::new(-0.965 + SCENE_X_OFFSET, 0.051, -0.026),
        yaw:    1.069,
        pitch:  -0.042,
        radius: 0.193,
        rows:   &STEEP_VIEW_ROWS,
    },
    DemoView {
        key:    KeyCode::KeyB,
        title:  "SHARP-CORNER WINGS",
        focus:  Vec3::new(-1.413 + SCENE_X_OFFSET, 0.039, 0.007),
        yaw:    1.651,
        pitch:  -0.018,
        radius: 0.096,
        rows:   &WING_VIEW_ROWS,
    },
];

// Panels.

/// Shared width for the three upper-right boxes -- equal width, and a fixed
/// value so the info box's paragraphs wrap.
const PANEL_BOX_WIDTH: Px = Px(240.0);

const OIT_ON_INFO_TITLE: &str = "WHY MSAA IS OFF";
const OIT_ON_INFO_PARAGRAPHS: [&str; 3] = [
    "OIT fixes transparency ordering for the ground plane and blended text.",
    "OIT requires MSAA to be off, so geometry edges no longer get multisampling.",
    "With POST AA set to None, those geometry edges can still alias.",
];
const OIT_ON_POST_INFO_TITLE: &str = "BEST TRADEOFF";
const OIT_ON_POST_INFO_PARAGRAPHS: [&str; 3] = [
    "OIT fixes transparency ordering for the ground plane and blended text.",
    "OIT requires MSAA to be off, so geometry edges no longer get multisampling.",
    "POST AA compensates for that by smoothing geometry edges after the frame is rendered.",
];
const OIT_OFF_INFO_TITLE: &str = "TRANSPARENCY ISSUES";
const OIT_OFF_INFO_PARAGRAPHS: [&str; 3] = [
    "With OIT off, transparent surfaces use depth-sorted blending.",
    "Our text renders best with AlphaMode::Blend, but then it can sort incorrectly against the ground plane. Turning OIT on solves this but MSAA is not compatible with OIT and has to be turned off.",
    "To compensate, the best solution is OIT to fix transparency ordering and POST AA to fix geometry edges.",
];
const OIT_OFF_NO_POST_INFO_PARAGRAPHS: [&str; 3] = [
    "With OIT off, transparent surfaces use depth-sorted blending.",
    "Our text renders best with AlphaMode::Blend, but then it can sort incorrectly against the ground plane. Turning OIT on solves this but MSAA is not compatible with OIT and has to be turned off.",
    "With OIT off, our code restores MSAA, which is why most geometry edges still look good. It still cannot fix transparency ordering.",
];

/// Bottom-left control panel -- column headers, geometry, and chip colors.
const TEXT_COLUMN_HEADER: &str = "TEXT (shader)";
const POST_COLUMN_HEADER: &str = "POST (Bevy)";
const PANEL_PADDING: Px = Px(10.0);
const PANEL_RADIUS: Px = Px(10.0);
const PANEL_BORDER_WIDTH: Px = Px(1.0);
const COLUMN_GAP: Px = Px(16.0);
const ROW_GAP: Px = Px(4.0);
const HEADER_COLOR: Color = Color::srgb(0.55, 0.78, 0.95);
const ACTIVE_COLOR: Color = Color::srgb(1.0, 0.9, 0.25);
const INACTIVE_COLOR: Color = Color::srgba(0.68, 0.72, 0.82, 0.9);
const PANEL_BORDER_COLOR: Color = Color::srgba(0.15, 0.7, 0.9, 0.4);

/// Gap between a row's keycap and its description.
const KEY_GAP: Px = Px(8.0);
/// Vertical gap between the stacked upper-right instruction boxes.
const PANEL_STACK_GAP: Px = Px(8.0);

// Scene support.

/// Ground plane sized to frame the word, squashed in depth and pushed back so
/// the word sits near the front edge while the rest recedes behind it.
const GROUND_SIZE: f32 = 4.2;
const GROUND_DEPTH_SCALE: f32 = 0.5;
/// Floor front edge sits this far in front (`+z`) of the text plane.
const GROUND_FRONT_MARGIN: f32 = 0.2;
const GROUND_CENTER_Z: f32 =
    DISPLAY_Z + GROUND_FRONT_MARGIN - GROUND_SIZE * GROUND_DEPTH_SCALE * 0.5;
const GROUND_Y: f32 = -0.15;

/// Frontal key/fill rig matching `typography.rs`: aim at the word and place the
/// key light high and far in front (`+z`) so shadows trail behind the glyphs.
const LIGHT_AIM: Vec3 = Vec3::new(0.0, HEADLINE_Y, DISPLAY_Z);
const KEY_LIGHT_ILLUMINANCE: f32 = 6_000.0;
const KEY_LIGHT_POS: Vec3 = Vec3::new(0.0, 5.0, DISPLAY_Z + 12.0);

/// A slowly tumbling transparent cube to the right of the word. The cube gets
/// no in-shader AA, so its hard edges show what POST AA or MSAA do.
const CUBE_SIZE: f32 = 0.34;
const CUBE_X: f32 = 2.05;
const CUBE_COLOR: Color = Color::srgba(1.0, 0.08, 0.04, 1.0);
const CUBE_SPIN_SPEED: f32 = 0.35;
const CUBE_FACE_TEXT_OFFSET: f32 = 0.003;
const CUBE_FACE_PANEL_OFFSET: f32 = CUBE_SIZE * 0.5 + CUBE_FACE_TEXT_OFFSET;
const CUBE_COMPAT_PANEL_SIZE: f32 = CUBE_SIZE;
const CUBE_COMPAT_PANEL_FONT_SIZE: f32 = 38.0;
const CUBE_COMPAT_PANEL_PADDING: f32 = 0.01;
const CUBE_STATUS_PANEL_FONT_SIZE: f32 = 42.0;
const CUBE_STATUS_PANEL_ROW_GAP: f32 = 0.008;
const OIT_COMPAT_MESSAGE: &str = "MSAA is incompatible with OIT";
const TAA_COMPAT_MESSAGE: &str = "MSAA is incompatible with TAA";
const OIT_TAA_COMPAT_MESSAGE: &str = "MSAA is incompatible with OIT and TAA";

fn main() {
    // `bevy_diegetic::DiegeticUiPlugin` is registered automatically by
    // `fairy_dust::sprinkle_example`. It registers `TextAntiAlias`, so this
    // example only selects it.
    fairy_dust::sprinkle_example()
        .with_brp_extras()
        .with_save_window_position()
        .with_studio_lighting()
        .aim_at(LIGHT_AIM)
        .key_light_pos(KEY_LIGHT_POS)
        .key_light_illuminance(KEY_LIGHT_ILLUMINANCE)
        .with_ground_plane()
        .size(GROUND_SIZE)
        .transform(
            Transform::from_xyz(0.0, GROUND_Y, GROUND_CENTER_Z).with_scale(Vec3::new(
                1.0,
                1.0,
                GROUND_DEPTH_SCALE,
            )),
        )
        .insert(CameraHomeTarget)
        .with_orbit_cam(
            |_| {},
            OrbitCamInputMode::Preset(OrbitCamPreset::BlenderLike),
        )
        // OIT keeps the coplanar blended geometry sorted stably as the camera
        // orbits, and forces `Msaa::Off` on the cameras it manages.
        .with_stable_transparency()
        .with_camera_home()
        .pitch(HOME_PITCH)
        .yaw(HOME_YAW)
        .margin(HOME_FIT_MARGIN)
        .duration(Duration::from_millis(HOME_FIT_DURATION_MS))
        .with_title_bar(
            TitleBar::new()
                .with_title(EXAMPLE_TITLE)
                .active_control(OIT_CONTROL),
        )
        .wire_chip_to_state::<OitState, _>(OIT_CONTROL, |state| {
            if state.0 {
                ControlActivation::Active
            } else {
                ControlActivation::Inactive
            }
        })
        .with_camera_control_panel()
        .init_resource::<PostAa>()
        .init_resource::<OitState>()
        .add_systems(Startup, (setup, spawn_aa_panel, spawn_demo_panel))
        .add_systems(
            Update,
            (
                select_text_aa,
                select_post_aa,
                select_demo_view,
                toggle_oit,
                rotate_cube,
                refresh_aa_panel,
                refresh_demo_panel,
                refresh_cube_status_panels,
                refresh_cube_compatibility_panels,
            ),
        )
        .add_systems(PostUpdate, sync_taa_msaa)
        .run();
}

// =============================================================================
// TEXT AND AA -- the text scene plus the controls that switch AA behavior.
// Read this section to see how the example selects renderer and camera AA modes.
// =============================================================================

/// Which post-process anti-aliasing pass is active on the camera. The four
/// states are mutually exclusive -- selecting one removes the others.
#[derive(Resource, Clone, Copy, Default, PartialEq, Eq)]
enum PostAa {
    /// No post-process pass; the in-shader text AA carries the frame alone.
    #[default]
    None,
    /// SMAA: image-space luma-edge pass.
    Smaa,
    /// FXAA: cheaper image-space luma-edge pass.
    Fxaa,
    /// TAA: temporal blend across frames; adds the depth/motion prepasses.
    Taa,
}

/// Source of truth for the OIT toggle, mirrored into the `O OIT` title-bar chip.
#[derive(Resource, Clone, Copy, PartialEq, Eq)]
struct OitState(bool);

impl Default for OitState {
    fn default() -> Self { Self(true) }
}

fn setup(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    oit: Res<OitState>,
    post: Res<PostAa>,
) {
    commands.spawn((
        Name::new("Headline"),
        CameraHomeTarget,
        WorldText::new(HEADLINE_TEXT),
        WorldTextStyle::new(HEADLINE_SIZE).with_color(TEXT_COLOR),
        Transform::from_xyz(SCENE_X_OFFSET, HEADLINE_Y, DISPLAY_Z),
    ));
    commands.spawn((
        Name::new("Small line"),
        CameraHomeTarget,
        WorldText::new(SMALL_TEXT),
        WorldTextStyle::new(SMALL_SIZE).with_color(TEXT_COLOR),
        Transform::from_xyz(SCENE_X_OFFSET, SMALL_Y, DISPLAY_Z),
    ));
    // Transparent hard-edged geometry gives the post-process passes and MSAA a
    // visible target that is independent from the text shader's coverage AA.
    let cube = commands
        .spawn((
            Name::new("Spinning cube"),
            SpinningCube,
            Mesh3d(meshes.add(Cuboid::from_length(CUBE_SIZE))),
            MeshMaterial3d(materials.add(StandardMaterial {
                base_color: CUBE_COLOR,
                alpha_mode: AlphaMode::Opaque,
                ..default()
            })),
            Transform::from_xyz(CUBE_X + SCENE_X_OFFSET, HEADLINE_Y, DISPLAY_Z),
        ))
        .id();
    commands.entity(cube).with_children(|cube| {
        spawn_cube_status_panels(cube, CubeStatusSnapshot::new(Msaa::Off, oit.0, *post));
        spawn_cube_compatibility_panels(cube, oit.0, *post);
    });
}

fn spawn_cube_status_panels(cube: &mut ChildSpawnerCommands, snapshot: CubeStatusSnapshot) {
    let panel = cube_status_panel(snapshot);

    match panel {
        Ok(panel) => {
            let front_transform = Transform::from_xyz(0.0, 0.0, CUBE_FACE_PANEL_OFFSET);
            let back_transform = Transform::from_xyz(0.0, 0.0, -CUBE_FACE_PANEL_OFFSET)
                .with_rotation(Quat::from_rotation_y(std::f32::consts::PI));
            let top_transform = Transform::from_xyz(0.0, CUBE_FACE_PANEL_OFFSET, 0.0)
                .with_rotation(Quat::from_rotation_x(-std::f32::consts::FRAC_PI_2));
            let bottom_transform = Transform::from_xyz(0.0, -CUBE_FACE_PANEL_OFFSET, 0.0)
                .with_rotation(Quat::from_rotation_x(std::f32::consts::FRAC_PI_2));

            for transform in [
                front_transform,
                back_transform,
                top_transform,
                bottom_transform,
            ] {
                cube.spawn((
                    Name::new("Cube render status panel"),
                    CubeStatusPanel,
                    panel.clone(),
                    transform,
                ));
            }
        },
        Err(error) => {
            error!("aa_text: failed to build cube status panel: {error}");
        },
    }
}

fn spawn_cube_compatibility_panels(
    cube: &mut ChildSpawnerCommands,
    oit_enabled: bool,
    post: PostAa,
) {
    let transparent = cube_panel_material();
    let panel = DiegeticPanel::world()
        .size(CUBE_COMPAT_PANEL_SIZE, CUBE_COMPAT_PANEL_SIZE)
        .font_unit(Unit::Millimeters)
        .anchor(Anchor::Center)
        .material(transparent.clone())
        .text_material(transparent)
        .with_tree(build_cube_compatibility_tree(cube_compatibility_message(
            oit_enabled,
            post,
        )))
        .build();

    match panel {
        Ok(panel) => {
            let right_transform = Transform::from_xyz(CUBE_FACE_PANEL_OFFSET, 0.0, 0.0)
                .with_rotation(Quat::from_rotation_y(std::f32::consts::FRAC_PI_2));
            let left_transform = Transform::from_xyz(-CUBE_FACE_PANEL_OFFSET, 0.0, 0.0)
                .with_rotation(Quat::from_rotation_y(-std::f32::consts::FRAC_PI_2));

            cube.spawn((
                Name::new("Cube compatibility panel"),
                CubeCompatibilityPanel,
                panel.clone(),
                right_transform,
            ));
            cube.spawn((
                Name::new("Cube compatibility panel"),
                CubeCompatibilityPanel,
                panel,
                left_transform,
            ));
        },
        Err(error) => {
            error!("aa_text: failed to build cube compatibility panel: {error}");
        },
    }
}

/// On `1`-`4`, select the matching [`TextAntiAlias`] mode.
fn select_text_aa(keyboard: Res<ButtonInput<KeyCode>>, mut aa: ResMut<TextAntiAlias>) {
    for (key, _, mode) in TEXT_MODES {
        if keyboard.just_pressed(key) {
            if *aa != mode {
                *aa = mode;
            }
            return;
        }
    }
}

/// On `N`/`S`/`F`/`T`, select that post-process mode and reconcile the camera's
/// components.
fn select_post_aa(
    keyboard: Res<ButtonInput<KeyCode>>,
    mut mode: ResMut<PostAa>,
    cameras: Query<Entity, With<OrbitCam>>,
    mut commands: Commands,
) {
    for (key, _, selected) in POST_MODES {
        if !keyboard.just_pressed(key) {
            continue;
        }
        if *mode != selected {
            *mode = selected;
            for camera in &cameras {
                apply_post_aa(&mut commands, camera, selected);
            }
        }
        return;
    }
}

/// Strips every post-process pass off `camera`, then installs the one `mode`
/// selects. The frozen [`TemporalJitter`]/[`MipBias`] TAA leaves behind are
/// removed so the off-state renders unshifted.
fn apply_post_aa(commands: &mut Commands, camera: Entity, mode: PostAa) {
    let mut entity = commands.entity(camera);
    entity
        .remove::<Smaa>()
        .remove::<Fxaa>()
        .remove::<TemporalAntiAliasing>()
        .remove::<TemporalJitter>()
        .remove::<MipBias>();
    match mode {
        PostAa::None => {},
        PostAa::Smaa => {
            entity.insert(Smaa::default());
        },
        PostAa::Fxaa => {
            entity.insert(Fxaa::default());
        },
        PostAa::Taa => {
            entity.insert(TemporalAntiAliasing::default());
        },
    }
}

/// On `O`, toggle OIT on the scene camera. Inserting [`StableTransparency`]
/// turns OIT on; removing it lets the observer restore default MSAA.
fn toggle_oit(
    keyboard: Res<ButtonInput<KeyCode>>,
    mut state: ResMut<OitState>,
    cameras: Query<Entity, With<OrbitCam>>,
    mut commands: Commands,
) {
    if !keyboard.just_pressed(KeyCode::KeyO) {
        return;
    }
    state.0 = !state.0;
    for camera in &cameras {
        if state.0 {
            commands.entity(camera).insert(StableTransparency);
        } else {
            commands.entity(camera).remove::<StableTransparency>();
        }
    }
}

/// Keeps TAA's `Msaa::Off` requirement true when OIT is disabled. Leaving TAA
/// while OIT is still off restores normal MSAA on every camera.
fn sync_taa_msaa(
    mode: Res<PostAa>,
    oit: Res<OitState>,
    cameras: Query<(Entity, Option<&Msaa>), With<Camera>>,
    mut commands: Commands,
) {
    if oit.0 {
        return;
    }

    let desired = if *mode == PostAa::Taa {
        Msaa::Off
    } else {
        Msaa::default()
    };
    for (camera, msaa) in &cameras {
        if msaa != Some(&desired) {
            commands.entity(camera).insert(desired);
        }
    }
}

// =============================================================================
// CAMERA VIEWS -- authored comparison angles for the text AA artifacts.
// This supports the demo; it is not required to use `TextAntiAlias`.
// =============================================================================

/// A camera viewpoint `A` / `B` can fly to, paired with the instruction box that
/// names the comparison to run once you arrive.
struct DemoView {
    /// Key that flies the camera to this view.
    key:    KeyCode,
    /// Heading of this view's instruction box.
    title:  &'static str,
    /// Target orbit focus point (world units).
    focus:  Vec3,
    /// Target yaw and pitch (radians) and orbital radius (world units).
    yaw:    f32,
    pitch:  f32,
    radius: f32,
    /// Instruction rows: a leading keycap and its description.
    rows:   &'static [(&'static str, &'static str)],
}

impl DemoView {
    /// The single eased orbital glide the key plays to reach this view.
    const fn camera_move(&self) -> CameraMove {
        CameraMove::ToOrbit {
            focus:    self.focus,
            yaw:      self.yaw,
            pitch:    self.pitch,
            radius:   self.radius,
            duration: Duration::from_millis(DEMO_MOVE_DURATION_MS),
            easing:   EaseFunction::CubicInOut,
        }
    }
}

/// On `A` / `B`, animate every orbit camera to that demo's viewpoint. Skipped
/// while Ctrl+Shift are both held so the gizmo chord does not also trigger `A`.
fn select_demo_view(
    keyboard: Res<ButtonInput<KeyCode>>,
    cameras: Query<Entity, With<OrbitCam>>,
    mut commands: Commands,
) {
    if keyboard.any_pressed([KeyCode::ControlLeft, KeyCode::ControlRight])
        && keyboard.any_pressed([KeyCode::ShiftLeft, KeyCode::ShiftRight])
    {
        return;
    }
    for demo in &DEMO_VIEWS {
        if !keyboard.just_pressed(demo.key) {
            continue;
        }
        for camera in &cameras {
            commands.trigger(PlayAnimation::new(camera, [demo.camera_move()]));
        }
        return;
    }
}

// =============================================================================
// PANELS -- explanatory controls and state readouts for the example.
// This supports the demo; it is not required to use the renderer APIs.
// =============================================================================

/// Marker for the bottom-left two-column AA control panel.
#[derive(Component)]
struct AaPanel;

/// Marker for the upper-right instruction panel that stacks both demo boxes.
#[derive(Component)]
struct DemoPanel;

/// The three text styles a control column draws with: its header, an active
/// chip, and an inactive chip.
struct ColumnStyles {
    header:   LayoutTextStyle,
    active:   LayoutTextStyle,
    inactive: LayoutTextStyle,
}

/// Spawns the bottom-left panel with the initial state of both AA settings.
fn spawn_aa_panel(mut commands: Commands, aa: Res<TextAntiAlias>, post: Res<PostAa>) {
    let unlit = StandardMaterial {
        unlit: true,
        ..default_panel_material()
    };
    let panel = DiegeticPanel::screen()
        .size(Fit, Fit)
        .anchor(Anchor::BottomLeft)
        .material(unlit.clone())
        .text_material(unlit)
        .with_tree(build_aa_tree(*aa, *post))
        .build();

    match panel {
        Ok(panel) => {
            commands.spawn((AaPanel, panel, Transform::default()));
        },
        Err(error) => {
            error!("aa_text: failed to build AA panel: {error}");
        },
    }
}

/// Spawns the upper-right instruction panel -- the two demo boxes stacked above
/// an info box whose copy follows the OIT toggle.
fn spawn_demo_panel(mut commands: Commands, oit: Res<OitState>, post: Res<PostAa>) {
    let unlit = StandardMaterial {
        unlit: true,
        ..default_panel_material()
    };
    let panel = DiegeticPanel::screen()
        .size(Fit, Fit)
        .anchor(Anchor::TopRight)
        .material(unlit.clone())
        .text_material(unlit)
        .with_tree(build_demo_panel_tree(oit.0, *post))
        .build();

    match panel {
        Ok(panel) => {
            commands.spawn((DemoPanel, panel, Transform::default()));
        },
        Err(error) => {
            error!("aa_text: failed to build demo panel: {error}");
        },
    }
}

/// Repaints the panel whenever either AA setting changes so the active chip in
/// each column tracks the live state.
fn refresh_aa_panel(
    aa: Res<TextAntiAlias>,
    post: Res<PostAa>,
    panel: Single<Entity, With<AaPanel>>,
    mut commands: Commands,
) {
    if !aa.is_changed() && !post.is_changed() {
        return;
    }
    commands.set_tree(*panel, build_aa_tree(*aa, *post));
}

/// Swaps the upper-right info copy when OIT or POST changes, so the panel
/// describes the current rendering tradeoff.
fn refresh_demo_panel(
    oit: Res<OitState>,
    post: Res<PostAa>,
    panel: Single<Entity, With<DemoPanel>>,
    mut commands: Commands,
) {
    if !oit.is_changed() && !post.is_changed() {
        return;
    }
    commands.set_tree(*panel, build_demo_panel_tree(oit.0, *post));
}

/// Builds the bottom-left panel tree: two columns, each chip highlighted when it
/// matches the live setting.
fn build_aa_tree(aa: TextAntiAlias, post: PostAa) -> LayoutTree {
    let mut builder = LayoutBuilder::with_root(El::new().width(Sizing::FIT).height(Sizing::FIT));
    build_aa_layout(&mut builder, aa, post);
    builder.build()
}

fn build_aa_layout(builder: &mut LayoutBuilder, aa: TextAntiAlias, post: PostAa) {
    let styles = ColumnStyles {
        header:   LayoutTextStyle::new(TITLE_SIZE)
            .with_color(HEADER_COLOR)
            .no_wrap(),
        active:   LayoutTextStyle::new(LABEL_SIZE)
            .with_color(ACTIVE_COLOR)
            .no_wrap(),
        inactive: LayoutTextStyle::new(LABEL_SIZE)
            .with_color(INACTIVE_COLOR)
            .no_wrap(),
    };
    builder.with(
        El::new()
            .width(Sizing::FIT)
            .height(Sizing::FIT)
            .direction(Direction::LeftToRight)
            .child_gap(COLUMN_GAP)
            .padding(Padding::all(PANEL_PADDING))
            .corner_radius(CornerRadius::all(PANEL_RADIUS))
            .background(DEFAULT_PANEL_BACKGROUND)
            .border(Border::all(PANEL_BORDER_WIDTH, PANEL_BORDER_COLOR)),
        |builder| {
            build_column(
                builder,
                TEXT_COLUMN_HEADER,
                TEXT_MODES
                    .into_iter()
                    .map(|(_, label, mode)| (label, mode == aa)),
                &styles,
            );
            panel_divider_vertical(builder);
            build_column(
                builder,
                POST_COLUMN_HEADER,
                POST_MODES
                    .into_iter()
                    .map(|(_, label, mode)| (label, mode == post)),
                &styles,
            );
        },
    );
}

/// Draws one labeled column: a header followed by one chip per row, each chip
/// using the active style when its `bool` is set.
fn build_column<'a>(
    builder: &mut LayoutBuilder,
    header: &str,
    rows: impl IntoIterator<Item = (&'a str, bool)>,
    styles: &ColumnStyles,
) {
    builder.with(
        El::new()
            .width(Sizing::FIT)
            .height(Sizing::FIT)
            .direction(Direction::TopToBottom)
            .child_gap(ROW_GAP),
        |builder| {
            builder.text(header, styles.header.clone());
            panel_divider(builder);
            for (label, active) in rows {
                let style = if active {
                    &styles.active
                } else {
                    &styles.inactive
                };
                builder.text(label, style.clone());
            }
        },
    );
}

/// Builds the upper-right instruction panel tree: a transparent, right-aligned
/// column stacking one bordered box per [`DemoView`].
fn build_demo_panel_tree(oit_enabled: bool, post: PostAa) -> LayoutTree {
    let mut builder = LayoutBuilder::with_root(El::new().width(Sizing::FIT).height(Sizing::FIT));
    build_demo_panel_layout(&mut builder, oit_enabled, post);
    builder.build()
}

fn build_demo_panel_layout(builder: &mut LayoutBuilder, oit_enabled: bool, post: PostAa) {
    let title = LayoutTextStyle::new(TITLE_SIZE)
        .with_color(HEADER_COLOR)
        .no_wrap();
    let key = LayoutTextStyle::new(LABEL_SIZE)
        .with_color(ACTIVE_COLOR)
        .no_wrap();
    let body = LayoutTextStyle::new(LABEL_SIZE)
        .with_color(INACTIVE_COLOR)
        .no_wrap();
    // Fixed-width container plus GROW boxes line the three boxes up on both
    // edges and give the info box's paragraphs a stable wrap width.
    builder.with(
        El::new()
            .width(Sizing::fixed(PANEL_BOX_WIDTH))
            .height(Sizing::FIT)
            .direction(Direction::TopToBottom)
            .child_gap(PANEL_STACK_GAP),
        |builder| {
            for demo in &DEMO_VIEWS {
                build_demo_box(builder, demo, &title, &key, &body);
            }
            build_info_box(builder, &title, oit_enabled, post);
        },
    );
}

/// Draws one demo's bordered box: its title over one row per key, each row
/// pairing a highlighted keycap with its description.
fn build_demo_box(
    builder: &mut LayoutBuilder,
    demo: &DemoView,
    title: &LayoutTextStyle,
    key: &LayoutTextStyle,
    body: &LayoutTextStyle,
) {
    builder.with(
        El::new()
            .width(Sizing::GROW)
            .height(Sizing::FIT)
            .direction(Direction::TopToBottom)
            .child_gap(ROW_GAP)
            .padding(Padding::all(PANEL_PADDING))
            .corner_radius(CornerRadius::all(PANEL_RADIUS))
            .background(DEFAULT_PANEL_BACKGROUND)
            .border(Border::all(PANEL_BORDER_WIDTH, PANEL_BORDER_COLOR)),
        |builder| {
            builder.text(demo.title, title.clone());
            panel_divider(builder);
            for &(keycap, description) in demo.rows {
                builder.with(
                    El::new()
                        .width(Sizing::FIT)
                        .height(Sizing::FIT)
                        .direction(Direction::LeftToRight)
                        .child_gap(KEY_GAP),
                    |builder| {
                        builder.text(keycap, key.clone());
                        builder.text(description, body.clone());
                    },
                );
            }
        },
    );
}

/// Draws the caption box: a title, a divider rule, then the OIT-state explainer,
/// wrapped to the box width.
fn build_info_box(
    builder: &mut LayoutBuilder,
    title: &LayoutTextStyle,
    oit_enabled: bool,
    post: PostAa,
) {
    // Wrapped text lets each paragraph flow to the fixed box width.
    let body = LayoutTextStyle::new(LABEL_SIZE).with_color(INACTIVE_COLOR);
    let (info_title, paragraphs) = info_copy(oit_enabled, post);
    builder.with(
        El::new()
            .width(Sizing::GROW)
            .height(Sizing::FIT)
            .direction(Direction::TopToBottom)
            .child_gap(ROW_GAP)
            .padding(Padding::all(PANEL_PADDING))
            .corner_radius(CornerRadius::all(PANEL_RADIUS))
            .background(DEFAULT_PANEL_BACKGROUND)
            .border(Border::all(PANEL_BORDER_WIDTH, PANEL_BORDER_COLOR)),
        |builder| {
            builder.text(info_title, title.clone());
            panel_divider(builder);
            for paragraph in paragraphs {
                builder.text(*paragraph, body.clone());
            }
        },
    );
}

const fn info_copy(oit_enabled: bool, post: PostAa) -> (&'static str, &'static [&'static str]) {
    match (oit_enabled, post) {
        (true, PostAa::None) => (OIT_ON_INFO_TITLE, &OIT_ON_INFO_PARAGRAPHS),
        (true, _) => (OIT_ON_POST_INFO_TITLE, &OIT_ON_POST_INFO_PARAGRAPHS),
        (false, PostAa::None) => (OIT_OFF_INFO_TITLE, &OIT_OFF_NO_POST_INFO_PARAGRAPHS),
        (false, _) => (OIT_OFF_INFO_TITLE, &OIT_OFF_INFO_PARAGRAPHS),
    }
}

/// A horizontal hairline rule spanning the panel width, drawn under titles and
/// column headers.
fn panel_divider(builder: &mut LayoutBuilder) {
    builder.with(
        El::new()
            .width(Sizing::GROW)
            .height(Sizing::fixed(Px(1.0)))
            .background(PANEL_BORDER_COLOR),
        |_| {},
    );
}

/// A vertical hairline rule spanning the panel height, drawn between columns.
fn panel_divider_vertical(builder: &mut LayoutBuilder) {
    builder.with(
        El::new()
            .width(Sizing::fixed(Px(1.0)))
            .height(Sizing::GROW)
            .background(PANEL_BORDER_COLOR),
        |_| {},
    );
}

// =============================================================================
// SCENE SUPPORT -- lighting, ground placement, and decorative hard-edge motion.
// This gives the AA modes realistic geometry and transparency to work against.
// =============================================================================

/// Marker for the slowly tumbling cube right of the word.
#[derive(Component)]
struct SpinningCube;

/// Marker for the render-status panel cloned onto four cube faces.
#[derive(Component)]
struct CubeStatusPanel;

/// Marker for a cube-side compatibility message panel.
#[derive(Component, Clone, Copy)]
struct CubeCompatibilityPanel;

#[derive(Clone, Copy, PartialEq, Eq)]
struct CubeStatusSnapshot {
    msaa:        Msaa,
    oit_enabled: bool,
    post:        PostAa,
}

impl CubeStatusSnapshot {
    const fn new(msaa: Msaa, oit_enabled: bool, post: PostAa) -> Self {
        Self {
            msaa,
            oit_enabled,
            post,
        }
    }
}

const fn cube_compatibility_message(oit_enabled: bool, post: PostAa) -> Option<&'static str> {
    match (oit_enabled, post) {
        (true, PostAa::Taa) => Some(OIT_TAA_COMPAT_MESSAGE),
        (true, _) => Some(OIT_COMPAT_MESSAGE),
        (false, PostAa::Taa) => Some(TAA_COMPAT_MESSAGE),
        (false, _) => None,
    }
}

const fn msaa_label(msaa: Msaa) -> &'static str {
    match msaa {
        Msaa::Off => "MSAA Off",
        Msaa::Sample2 => "MSAA 2x",
        Msaa::Sample4 => "MSAA 4x",
        Msaa::Sample8 => "MSAA 8x",
    }
}

const fn oit_label(oit_enabled: bool) -> &'static str {
    if oit_enabled {
        "OIT On"
    } else {
        "OIT Off"
    }
}

const fn post_label(post: PostAa) -> &'static str {
    match post {
        PostAa::None => "Post None",
        PostAa::Smaa => "Post SMAA",
        PostAa::Fxaa => "Post FXAA",
        PostAa::Taa => "Post TAA",
    }
}

fn cube_panel_material() -> StandardMaterial {
    StandardMaterial {
        base_color: Color::NONE,
        alpha_mode: AlphaMode::Blend,
        unlit: true,
        ..default_panel_material()
    }
}

fn cube_status_panel(snapshot: CubeStatusSnapshot) -> Result<DiegeticPanel, InvalidSize> {
    let transparent = cube_panel_material();
    DiegeticPanel::world()
        .size(CUBE_COMPAT_PANEL_SIZE, CUBE_COMPAT_PANEL_SIZE)
        .font_unit(Unit::Millimeters)
        .anchor(Anchor::Center)
        .material(transparent.clone())
        .text_material(transparent)
        .with_tree(build_cube_status_tree(snapshot))
        .build()
}

fn build_cube_status_tree(snapshot: CubeStatusSnapshot) -> LayoutTree {
    let mut builder = LayoutBuilder::with_root(
        El::new()
            .width(Sizing::fixed(CUBE_COMPAT_PANEL_SIZE))
            .height(Sizing::fixed(CUBE_COMPAT_PANEL_SIZE))
            .direction(Direction::TopToBottom)
            .child_alignment(AlignX::Center, AlignY::Center)
            .child_gap(CUBE_STATUS_PANEL_ROW_GAP)
            .padding(Padding::all(CUBE_COMPAT_PANEL_PADDING))
            .clip(),
    );
    let style = LayoutTextStyle::new(CUBE_STATUS_PANEL_FONT_SIZE)
        .with_color(Color::WHITE)
        .with_align(TextAlign::Center)
        .with_shadow_mode(GlyphShadowMode::None);
    for label in [
        msaa_label(snapshot.msaa),
        oit_label(snapshot.oit_enabled),
        post_label(snapshot.post),
    ] {
        builder.text(label, style.clone());
    }
    builder.build()
}

fn build_cube_compatibility_tree(message: Option<&str>) -> LayoutTree {
    let mut builder = LayoutBuilder::with_root(
        El::new()
            .width(Sizing::fixed(CUBE_COMPAT_PANEL_SIZE))
            .height(Sizing::fixed(CUBE_COMPAT_PANEL_SIZE))
            .direction(Direction::TopToBottom)
            .child_alignment(AlignX::Center, AlignY::Center)
            .padding(Padding::all(CUBE_COMPAT_PANEL_PADDING))
            .clip(),
    );
    if let Some(message) = message {
        builder.text(
            message,
            LayoutTextStyle::new(CUBE_COMPAT_PANEL_FONT_SIZE)
                .with_color(Color::WHITE)
                .with_align(TextAlign::Center)
                .with_shadow_mode(GlyphShadowMode::None),
        );
    }
    builder.build()
}

/// Updates the cube-face status panels from the actual `Msaa` component on the
/// main orbit camera plus the user-selected OIT and post-process modes.
fn refresh_cube_status_panels(
    cameras: Query<Option<&Msaa>, (With<OrbitCam>, With<Camera>)>,
    oit: Res<OitState>,
    post: Res<PostAa>,
    panels: Query<Entity, With<CubeStatusPanel>>,
    mut last_snapshot: Local<Option<CubeStatusSnapshot>>,
    mut commands: Commands,
) {
    let fallback_msaa = if oit.0 || *post == PostAa::Taa {
        Msaa::Off
    } else {
        Msaa::default()
    };
    let msaa = cameras
        .iter()
        .next()
        .flatten()
        .copied()
        .unwrap_or(fallback_msaa);
    let snapshot = CubeStatusSnapshot::new(msaa, oit.0, *post);
    if last_snapshot.is_some_and(|previous| previous == snapshot) {
        return;
    }
    *last_snapshot = Some(snapshot);
    for entity in &panels {
        commands.set_tree(entity, build_cube_status_tree(snapshot));
    }
}

/// Rebuilds the invisible cube-side panels when their compatibility state
/// changes.
fn refresh_cube_compatibility_panels(
    oit: Res<OitState>,
    post: Res<PostAa>,
    panels: Query<Entity, With<CubeCompatibilityPanel>>,
    mut commands: Commands,
) {
    if !oit.is_changed() && !post.is_changed() {
        return;
    }
    for entity in &panels {
        commands.set_tree(
            entity,
            build_cube_compatibility_tree(cube_compatibility_message(oit.0, *post)),
        );
    }
}

/// Tumbles the cube slowly about a tilted axis so its edges sweep through a
/// range of angles.
fn rotate_cube(time: Res<Time>, mut cubes: Query<&mut Transform, With<SpinningCube>>) {
    let angle = CUBE_SPIN_SPEED * time.delta_secs();
    let axis = Vec3::new(0.3, 1.0, 0.0).normalize();
    for mut transform in &mut cubes {
        transform.rotate(Quat::from_axis_angle(axis, angle));
    }
}
