//! `aa_text` — compare every anti-aliasing path the text renderer can use, over
//! plain unlit text with nothing else in the scene.
//!
//! slug renders glyph edges as analytic alpha coverage, sampled once per pixel.
//! At grazing angles that single sample can't represent the foreshortened pixel
//! footprint, so edges stair-step. There are two fundamentally different places
//! to fix it — the two columns of the bottom-left panel.
//!
//! **In the coverage shader — [`TextAntiAlias`] (keys `1`–`4`).** slug
//! anti-aliases glyph edges inside the fragment shader: no extra pass, and it
//! survives OIT (which forces `Msaa::Off`). The setting combines two orthogonal
//! mechanisms:
//! - **Anisotropic band** — sizes the edge ramp from the distance gradient so it holds ~1px per
//!   screen axis, fixing the convex-corner balloon at grazing angles that the scalar band
//!   over-widens into a wing.
//! - **Supersampling** — evaluates coverage at four sub-pixel sample points and averages, fixing
//!   the stepping along a shallow edge that a single sample can't resolve.
//!
//! The four modes are the points on that grid: `Off` (neither), `Aniso` (band
//! only), `Super` (samples only), `Both` (default, and the best result). The
//! cheaper modes exist for performance — the band is nearly free, but
//! supersampling evaluates coverage four (or five, combined) times per fragment,
//! so a text-dense frame can reclaim fill-rate by stepping down.
//!
//! **As a post-process pass over the resolved frame — keys `N` `S` `F` `T`.**
//! Mutually exclusive, with `None` the off state:
//! - **SMAA** — luma-edge detection in image space; keeps MSAA on.
//! - **FXAA** — cheaper, blurrier luma-edge pass; keeps MSAA on.
//! - **TAA** — temporal blend across frames; requires `Msaa::Off` plus the depth/motion prepasses.
//!   Included for completeness — note it ghosts on alpha-blended glyphs (the transparency the
//!   renderer exists to draw), so it is the weakest fit here even though it AA's the most.
//!
//! The text is unlit, so its color never varies as you orbit. Orbit to a grazing
//! angle (MMB) to see the artifacts, then select each mode.
//!
//! Hotkeys:
//! - `1` `2` `3` `4` — select the in-shader mode: Off / Aniso / Super / Both.
//! - `N` `S` `F` `T` — select the post-process pass: None / SMAA / FXAA / TAA.
//! - `H` — home the camera.

use std::time::Duration;

use bevy::anti_alias::fxaa::Fxaa;
use bevy::anti_alias::smaa::Smaa;
use bevy::anti_alias::taa::TemporalAntiAliasing;
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
use bevy_diegetic::GlyphShadowMode;
use bevy_diegetic::LayoutBuilder;
use bevy_diegetic::LayoutTextStyle;
use bevy_diegetic::LayoutTree;
use bevy_diegetic::Padding;
use bevy_diegetic::Px;
use bevy_diegetic::Sizing;
use bevy_diegetic::TextAntiAlias;
use bevy_diegetic::WorldText;
use bevy_diegetic::WorldTextStyle;
use bevy_diegetic::default_panel_material;
use bevy_lagrange::OrbitCam;
use bevy_lagrange::OrbitCamInputMode;
use bevy_lagrange::OrbitCamPreset;
use fairy_dust::CameraHomeTarget;
use fairy_dust::DEFAULT_PANEL_BACKGROUND;
use fairy_dust::LABEL_SIZE;
use fairy_dust::TitleBar;

const EXAMPLE_TITLE: &str = "Anti-aliasing";
const HEADLINE_TEXT: &str = "Anti-aliasing";
const HEADLINE_SIZE: f32 = 0.40;
const HEADLINE_Y: f32 = 0.18;
const SMALL_TEXT: &str = "the quick brown fox jumps over the lazy dog";
const SMALL_SIZE: f32 = 0.05;
const SMALL_Y: f32 = -0.06;
const DISPLAY_Z: f32 = 0.0;
const TEXT_COLOR: Color = Color::srgb(0.92, 0.92, 0.94);

/// Fallback home region for the cube `fairy_dust` frames before a
/// [`CameraHomeTarget`] entity exists. The headline carries the marker, so the
/// camera frames the headline (and its glyph children) directly once its meshes
/// load; this region is only the pre-load placeholder.
const HOME_CENTER: Vec3 = Vec3::new(0.0, HEADLINE_Y, DISPLAY_Z);
const HOME_PITCH: f32 = 0.0;
const HOME_YAW: f32 = 0.0;
const HOME_FIT_MARGIN: f32 = 0.15;
const HOME_FIT_DURATION_MS: u64 = 900;

/// Bottom-left control panel — column headers, geometry, and chip colors.
const TEXT_COLUMN_HEADER: &str = "TEXT (shader)";
const POST_COLUMN_HEADER: &str = "POST (Bevy)";
const PANEL_PADDING: Px = Px(10.0);
const PANEL_RADIUS: Px = Px(10.0);
const PANEL_BORDER_WIDTH: Px = Px(1.0);
const COLUMN_GAP: Px = Px(28.0);
const ROW_GAP: Px = Px(4.0);
const HEADER_COLOR: Color = Color::srgb(0.55, 0.78, 0.95);
const ACTIVE_COLOR: Color = Color::srgb(1.0, 0.9, 0.25);
const INACTIVE_COLOR: Color = Color::srgba(0.68, 0.72, 0.82, 0.9);
const PANEL_BORDER_COLOR: Color = Color::srgba(0.15, 0.7, 0.9, 0.4);

/// The in-shader [`TextAntiAlias`] modes, in cost order. One source of truth for
/// both the key that selects each and the chip label shown for it.
const TEXT_MODES: [(KeyCode, &str, TextAntiAlias); 4] = [
    (KeyCode::Digit1, "1 Off", TextAntiAlias::Off),
    (KeyCode::Digit2, "2 Aniso", TextAntiAlias::Anisotropic),
    (KeyCode::Digit3, "3 Super", TextAntiAlias::Supersample),
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
    /// No post-process pass; rely on MSAA + the in-shader AA alone.
    #[default]
    None,
    /// SMAA: image-space luma-edge pass, MSAA stays on.
    Smaa,
    /// FXAA: cheaper image-space luma-edge pass, MSAA stays on.
    Fxaa,
    /// TAA: temporal blend; forces `Msaa::Off` and adds the prepasses.
    Taa,
}

/// Marker for the bottom-left two-column AA control panel.
#[derive(Component)]
struct AaPanel;

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
        .with_orbit_cam(
            |_| {},
            OrbitCamInputMode::Preset(OrbitCamPreset::BlenderLike),
        )
        .with_camera_home(Transform::from_translation(HOME_CENTER))
        .pitch(HOME_PITCH)
        .yaw(HOME_YAW)
        .margin(HOME_FIT_MARGIN)
        .duration(Duration::from_millis(HOME_FIT_DURATION_MS))
        .with_title_bar(TitleBar::new().with_title(EXAMPLE_TITLE))
        .with_camera_control_panel()
        .init_resource::<PostAa>()
        .add_systems(Startup, (setup, spawn_aa_panel))
        .add_systems(Update, (select_text_aa, select_post_aa, refresh_aa_panel))
        .run();
}

fn setup(mut commands: Commands) {
    commands.spawn((
        Name::new("Headline"),
        CameraHomeTarget,
        WorldText::new(HEADLINE_TEXT),
        WorldTextStyle::new(HEADLINE_SIZE)
            .with_color(TEXT_COLOR)
            .with_unlit()
            .with_shadow_mode(GlyphShadowMode::None),
        Transform::from_xyz(0.0, HEADLINE_Y, DISPLAY_Z),
    ));
    commands.spawn((
        Name::new("Small line"),
        WorldText::new(SMALL_TEXT),
        WorldTextStyle::new(SMALL_SIZE)
            .with_color(TEXT_COLOR)
            .with_unlit()
            .with_shadow_mode(GlyphShadowMode::None),
        Transform::from_xyz(0.0, SMALL_Y, DISPLAY_Z),
    ));
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
/// selects. TAA is the only mode that touches MSAA — it requires `Msaa::Off`;
/// all others keep `Msaa::default()`. The frozen [`TemporalJitter`]/[`MipBias`]
/// TAA leaves behind are removed so the off-state renders unshifted.
fn apply_post_aa(commands: &mut Commands, camera: Entity, mode: PostAa) {
    let mut entity = commands.entity(camera);
    entity
        .remove::<Smaa>()
        .remove::<Fxaa>()
        .remove::<TemporalAntiAliasing>()
        .remove::<TemporalJitter>()
        .remove::<MipBias>();
    match mode {
        PostAa::None => {
            entity.insert(Msaa::default());
        },
        PostAa::Smaa => {
            entity.insert((Msaa::default(), Smaa::default()));
        },
        PostAa::Fxaa => {
            entity.insert((Msaa::default(), Fxaa::default()));
        },
        PostAa::Taa => {
            entity.insert((Msaa::Off, TemporalAntiAliasing::default()));
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
