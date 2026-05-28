//! `aa_text` — compare every anti-aliasing path the text renderer can use, on
//! lit text standing on a ground plane under studio lighting.
//!
//! slug renders glyph edges as analytic alpha coverage, sampled once per pixel.
//! At grazing angles that single sample can't represent the foreshortened pixel
//! footprint. There are two fundamentally different places to fix it — the two
//! columns of the bottom-left panel.
//!
//! **In the coverage shader — [`TextAntiAlias`] (keys `1`–`4`).** slug
//! anti-aliases glyph edges inside the fragment shader: no extra pass, and it
//! survives OIT (which forces `Msaa::Off`). The setting combines two orthogonal
//! mechanisms:
//! - **Anisotropic band** — sizes the edge ramp from the distance gradient so it holds ~1px per
//!   screen axis, keeping glyph edges crisp at grazing angles where the scalar band otherwise
//!   widens the ramp into a blur.
//! - **Supersampling** — strides coverage samples along the foreshortened footprint axis (one
//!   head-on, more as the angle steepens). It erases the wing a single band sample leaves off a
//!   sharp convex corner — visible only on sharp convex corners at the most extreme viewing angles,
//!   a no-op everywhere else.
//!
//! The four modes are the points on that grid: `Off` (neither), `Anisotropic`
//! (band only), `SuperSample` (samples only), `Both` (default, and the best
//! result). The cheaper modes exist for performance — the band is nearly free,
//! while `Both` strides up to 16 samples per fragment at the steepest grazing
//! angles, so a text-dense frame can reclaim fill-rate by stepping down. To see
//! supersampling's effect, press `B` for the sharp-corner view and flip
//! `Anisotropic` ↔ `Both`.
//!
//! **As a post-process pass over the resolved frame — keys `N` `S` `F` `T`.**
//! Mutually exclusive, with `None` the off state. While OIT is on (the default)
//! the camera runs `Msaa::Off`, so these passes — not MSAA — anti-alias geometry
//! edges; toggle OIT off with `O` and MSAA returns (but TAA, which needs
//! `Msaa::Off`, then can't run):
//! - **SMAA** — luma-edge detection in image space.
//! - **FXAA** — cheaper, blurrier luma-edge pass.
//! - **TAA** — temporal blend across frames; adds the depth/motion prepasses. Included for
//!   completeness — note it ghosts on alpha-blended glyphs (the transparency the renderer exists to
//!   draw), so it is the weakest fit here even though it AA's the most.
//!
//! The text is lit by a studio key/fill rig and stands on a ground plane,
//! casting shadows — the now-fixed AA shown in a real scene rather than in
//! isolation. A slowly tumbling cube to the right of the word is opaque geometry
//! with no in-shader AA, so it shows what the POST passes (or MSAA, when OIT is
//! toggled off with `O`) do to hard edges. Orbit to a grazing angle (MMB), or
//! press `A` / `B`, to reach the text artifacts, then select each mode.
//!
//! Hotkeys:
//! - `1` `2` `3` `4` — select the in-shader mode: `Off` / `Anisotropic` / `SuperSample` / `Both`.
//! - `N` `S` `F` `T` — select the post-process pass: None / SMAA / FXAA / TAA.
//! - `A` — animate to a steep grazing angle (Off vs Anisotropic: edge blur vs crisp).
//! - `B` — animate to a sharp glyph corner (Anisotropic vs Both: the wing comparison).
//! - `O` — toggle OIT; off shows the coplanar sorting break and hands MSAA back to geometry.
//! - `H` — home the camera.

use std::time::Duration;

use bevy::anti_alias::fxaa::Fxaa;
use bevy::anti_alias::smaa::Smaa;
use bevy::anti_alias::taa::TemporalAntiAliasing;
use bevy::math::curve::easing::EaseFunction;
use bevy::prelude::*;
use bevy::render::camera::MipBias;
use bevy::render::camera::TemporalJitter;
use bevy_diegetic::Anchor;
use bevy_diegetic::Border;
use bevy_diegetic::CornerRadius;
use bevy_diegetic::DiegeticPanel;
use bevy_diegetic::DiegeticPanelCommands;
use bevy_diegetic::Direction;
use bevy_diegetic::El;
use bevy_diegetic::Fit;
use bevy_diegetic::LayoutBuilder;
use bevy_diegetic::LayoutTextStyle;
use bevy_diegetic::LayoutTree;
use bevy_diegetic::Padding;
use bevy_diegetic::Px;
use bevy_diegetic::Sizing;
use bevy_diegetic::StableTransparency;
use bevy_diegetic::TextAntiAlias;
use bevy_diegetic::WorldText;
use bevy_diegetic::WorldTextStyle;
use bevy_diegetic::default_panel_material;
use bevy_lagrange::CameraMove;
use bevy_lagrange::OrbitCam;
use bevy_lagrange::OrbitCamInputMode;
use bevy_lagrange::OrbitCamPreset;
use bevy_lagrange::PlayAnimation;
use fairy_dust::CameraHomeTarget;
use fairy_dust::ControlActivation;
use fairy_dust::DEFAULT_PANEL_BACKGROUND;
use fairy_dust::LABEL_SIZE;
use fairy_dust::TITLE_SIZE;
use fairy_dust::TitleBar;

const EXAMPLE_TITLE: &str = "Anti-Aliasing";
const HEADLINE_TEXT: &str = "Anti-Aliasing";
const HEADLINE_SIZE: f32 = 0.40;
const HEADLINE_Y: f32 = 0.18;
const SMALL_TEXT: &str = "the quick brown fox jumps over the lazy dog";
const SMALL_SIZE: f32 = 0.05;
const SMALL_Y: f32 = -0.06;
const DISPLAY_Z: f32 = 0.0;
const TEXT_COLOR: Color = Color::srgb(0.92, 0.92, 0.94);

/// Fallback home region for the cube `fairy_dust` frames before any
/// [`CameraHomeTarget`] entity has loaded its meshes. The headline, the small
/// line, and the ground plane all carry the marker, so once their meshes
/// land the camera frames the union of their AABBs (a box around all three —
/// dominated by the ground plane, so the framed view shows the floor with
/// the text centered over it), not this placeholder.
const HOME_CENTER: Vec3 = Vec3::new(0.0, HEADLINE_Y, DISPLAY_Z);
/// Opens looking down ~14° (`asin(Δy / radius)` of the framed pose) so the
/// ground plane tilts toward the camera and more of the cast shadow is visible.
/// Positive pitch puts the camera above the focus — see `orbital_math`.
const HOME_PITCH: f32 = 0.24;
const HOME_YAW: f32 = 0.0;
const HOME_FIT_MARGIN: f32 = 0.1;
const HOME_FIT_DURATION_MS: u64 = 900;

/// Ground plane sized to frame the word, the way `typography.rs` does it: a
/// plane proportional to the headline (not the canonical 8-unit default, which
/// dwarfs it), squashed in depth and pushed back so the word sits near the
/// *front* edge and the rest recedes behind it. Dropped just below the text so
/// the lit glyphs stand over it and cast shadows, leaving the text positions —
/// and the baked `A` / `B` views — untouched.
/// Horizontal shift applied to the headline, small line, and cube together
/// so their combined AABB straddles `x = 0` instead of leaning right. Picked
/// to center the actual measured combined extent — headline AABB
/// `-1.528..+1.548`, cube worst-case (after `sqrt(3) × half`) reaching
/// `+2.344` — whose midpoint is `+0.408`. The A/B demo-view focuses inherit
/// the same shift so they still point at the same letters.
const SCENE_X_OFFSET: f32 = -0.4;

const GROUND_SIZE: f32 = 4.2;
const GROUND_DEPTH_SCALE: f32 = 0.5;
/// Floor front edge sits this far in front (+z) of the text plane. Kept
/// small so most of the ground sits *behind* the text (in `-z`), catching
/// the headline + cube shadows the key light throws backward — without this
/// the headline's projection past the floor's back edge clips.
const GROUND_FRONT_MARGIN: f32 = 0.2;
const GROUND_CENTER_Z: f32 =
    DISPLAY_Z + GROUND_FRONT_MARGIN - GROUND_SIZE * GROUND_DEPTH_SCALE * 0.5;
const GROUND_Y: f32 = -0.15;

/// Frontal key/fill rig matching `typography.rs`: aim at the word and place the
/// key light high and far in front (+z) so the shadow direction
/// `(aim - key_light_pos)` trails straight back behind the glyphs, the way
/// typography casts them — rather than the default rig's off-axis light, which
/// throws shadows to the side.
const LIGHT_AIM: Vec3 = Vec3::new(0.0, HEADLINE_Y, DISPLAY_Z);
const KEY_LIGHT_POS: Vec3 = Vec3::new(0.0, 5.0, DISPLAY_Z + 12.0);

/// A slowly tumbling opaque cube to the right of the word (clear of the left-side
/// `A` / `B` closeups). Opaque geometry gets no in-shader AA, so its hard edges
/// show what the POST passes — or MSAA, when OIT is toggled off — do, and what
/// they leave jagged otherwise.
const CUBE_SIZE: f32 = 0.34;
const CUBE_X: f32 = 2.05;
const CUBE_COLOR: Color = Color::srgb(0.62, 0.64, 0.70);
const CUBE_SPIN_SPEED: f32 = 0.35;

/// Title-bar chip that toggles OIT (stable transparency), sitting beside `H Home`.
const OIT_CONTROL: &str = "O OIT";

/// Shared width for the three upper-right boxes — equal width, and a fixed value
/// so the info box's paragraphs wrap.
const PANEL_BOX_WIDTH: Px = Px(240.0);

/// Third upper-right box: a title, a divider, then the fundamental why. Wrapped
/// paragraphs rather than the demo boxes' fixed rows.
const INFO_TITLE: &str = "WHY MSAA IS OFF";
const INFO_PARAGRAPHS: [&str; 3] = [
    "MSAA is off because it is incompatible with Order Independent Transparency (OIT).",
    "Without OIT (press O) the shader does not sort the floor and text correctly.",
    "With OIT on, geometry edges like the cube and the plane rely on POST AA.",
];

/// A camera viewpoint `A` / `B` can fly to, paired with the instruction box that
/// names the comparison to run once you arrive. The two boxes stack in the
/// upper-right corner, `A` (the simpler edge-blur point) above `B` (the
/// sharp-corner wing, which also needs `SuperSample`).
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

const DEMO_MOVE_DURATION_MS: u64 = 1200;

/// `A` — at a steep grazing angle the scalar band widens the edge ramp into a
/// blur while the anisotropic band holds it crisp. Off vs Anisotropic makes the
/// point on its own. Orbit taken from the live view this was authored against.
const STEEP_VIEW_ROWS: [(&str, &str); 3] = [
    ("A", "go to this view"),
    ("1", "off: edges blur"),
    ("2", "Anisotropic: crisp"),
];

/// `B` — a sharper, closer grazing view onto the apex of the headline's leading
/// `A`, the convex corner where a single coverage sample leaves a "wing".
/// Anisotropic crisps the edge but the wing survives until `Both` supersamples.
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

/// Bottom-left control panel — column headers, geometry, and chip colors.
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

/// Upper-right instruction boxes — drawn at the shared [`TITLE_SIZE`] /
/// [`LABEL_SIZE`] like the other panels. Gap between a row's keycap and its
/// description.
const KEY_GAP: Px = Px(8.0);
/// Vertical gap between the two stacked instruction boxes.
const PANEL_STACK_GAP: Px = Px(8.0);

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

/// Which post-process anti-aliasing pass is active on the camera. The four
/// states are mutually exclusive — selecting one removes the others. Orthogonal
/// to [`TextAntiAlias`], which runs in the coverage shader regardless.
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
/// Starts on — the scene opens with stable transparency.
#[derive(Resource, Clone, Copy, PartialEq, Eq)]
struct OitState(bool);

impl Default for OitState {
    fn default() -> Self { Self(true) }
}

/// Marker for the slowly tumbling cube left of the word.
#[derive(Component)]
struct SpinningCube;

/// Marker for the bottom-left two-column AA control panel.
#[derive(Component)]
struct AaPanel;

/// Marker for the upper-right instruction panel that stacks both demo boxes.
#[derive(Component)]
struct DemoPanel;

/// The three text styles a control column draws with: its header, an active
/// (highlighted) chip, and an inactive chip.
struct ColumnStyles {
    header:   LayoutTextStyle,
    active:   LayoutTextStyle,
    inactive: LayoutTextStyle,
}

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
        // OIT keeps the coplanar blended geometry (the ground plane and the text)
        // sorted stably as the camera orbits. It also forces `Msaa::Off` on the
        // main and screen-space cameras alike, which is what lets TAA (which
        // requires `Msaa::Off`) toggle without the macOS MSAA-switch stall — see
        // `apply_post_aa`.
        .with_stable_transparency()
        .with_camera_home(Transform::from_translation(HOME_CENTER))
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
            ),
        )
        .run();
}

fn setup(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
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
    // Opaque cube to the right, floating with its center on the word's vertical
    // center so a corner never dips into the ground plane as it tumbles.
    commands.spawn((
        Name::new("Spinning cube"),
        SpinningCube,
        Mesh3d(meshes.add(Cuboid::from_length(CUBE_SIZE))),
        MeshMaterial3d(materials.add(StandardMaterial {
            base_color: CUBE_COLOR,
            ..default()
        })),
        Transform::from_xyz(CUBE_X + SCENE_X_OFFSET, HEADLINE_Y, DISPLAY_Z),
    ));
}

/// Tumbles the cube slowly about a tilted axis so its edges sweep through a
/// range of angles, giving the POST passes (and MSAA) varied hard edges to work.
fn rotate_cube(time: Res<Time>, mut cubes: Query<&mut Transform, With<SpinningCube>>) {
    let angle = CUBE_SPIN_SPEED * time.delta_secs();
    let axis = Vec3::new(0.3, 1.0, 0.0).normalize();
    for mut transform in &mut cubes {
        transform.rotate(Quat::from_axis_angle(axis, angle));
    }
}

/// On `O`, toggle OIT on the scene camera. Inserting [`StableTransparency`]
/// turns OIT on (and forces `Msaa::Off`); removing it restores `Msaa::default()`
/// — both via the `bevy_diegetic` observers, on the main and screen-space cameras
/// alike. With OIT off the coplanar ground plane and text sort view-dependently,
/// which is the point: it shows why the scene defaults to OIT.
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

/// Marker for the headline text whose rendered bounds define the home frame.
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

/// Spawns the upper-right instruction panel — the two demo boxes stacked. It is
/// static — the rows never change — so unlike the AA panel it needs no refresh
/// system.
fn spawn_demo_panel(mut commands: Commands) {
    let unlit = StandardMaterial {
        unlit: true,
        ..default_panel_material()
    };
    let panel = DiegeticPanel::screen()
        .size(Fit, Fit)
        .anchor(Anchor::TopRight)
        .material(unlit.clone())
        .text_material(unlit)
        .with_tree(build_demo_panel_tree())
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

/// On `1`–`4`, select the matching [`TextAntiAlias`] mode.
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

/// On `A` / `B`, animate every orbit camera to that demo's viewpoint so the
/// in-shader modes have a visible artifact to compare against. Skipped while
/// Ctrl+Shift are both held so the gizmo chord (Ctrl+Shift+A) doesn't also
/// trigger the `A` demo.
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
/// selects. MSAA is never touched here — OIT owns it: `Msaa::Off` while OIT is
/// on, flipped (on the main and screen-space cameras together) by the `O`
/// toggle. Keeping the post-pass selection MSAA-free is the point: a *per-camera*
/// MSAA mismatch is what stalls the macOS Metal surface, and changing both
/// cameras at once never leaves one. SMAA and FXAA run over the frame as-is; TAA
/// needs `Msaa::Off`, so it only anti-aliases while OIT is on. The frozen
/// [`TemporalJitter`]/[`MipBias`] TAA leaves behind are removed so the off-state
/// renders unshifted.
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

/// Builds the bottom-left panel tree: two columns, each chip highlighted when it
/// matches the live setting.
fn build_aa_tree(aa: TextAntiAlias, post: PostAa) -> LayoutTree {
    let mut builder = LayoutBuilder::with_root(El::new().width(Sizing::FIT).height(Sizing::FIT));
    build_aa_layout(&mut builder, aa, post);
    builder.build()
}

fn build_aa_layout(builder: &mut LayoutBuilder, aa: TextAntiAlias, post: PostAa) {
    let styles = ColumnStyles {
        header:   LayoutTextStyle::new(LABEL_SIZE)
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
fn build_demo_panel_tree() -> LayoutTree {
    let mut builder = LayoutBuilder::with_root(El::new().width(Sizing::FIT).height(Sizing::FIT));
    build_demo_panel_layout(&mut builder);
    builder.build()
}

fn build_demo_panel_layout(builder: &mut LayoutBuilder) {
    let title = LayoutTextStyle::new(TITLE_SIZE)
        .with_color(HEADER_COLOR)
        .no_wrap();
    let key = LayoutTextStyle::new(LABEL_SIZE)
        .with_color(ACTIVE_COLOR)
        .no_wrap();
    let body = LayoutTextStyle::new(LABEL_SIZE)
        .with_color(INACTIVE_COLOR)
        .no_wrap();
    // Fixed-width container + GROW boxes: the three line up on both edges, and
    // the fixed width gives the info box's paragraphs something to wrap against.
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
            build_info_box(builder, &title);
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

/// Draws the static caption box: a title, a divider rule, then the
/// [`INFO_PARAGRAPHS`] explainer, wrapped to the box width.
fn build_info_box(builder: &mut LayoutBuilder, title: &LayoutTextStyle) {
    // Wrapped (no `no_wrap`) so each paragraph flows to the fixed box width.
    let body = LayoutTextStyle::new(LABEL_SIZE).with_color(INACTIVE_COLOR);
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
            builder.text(INFO_TITLE, title.clone());
            panel_divider(builder);
            for paragraph in INFO_PARAGRAPHS {
                builder.text(paragraph, body.clone());
            }
        },
    );
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
