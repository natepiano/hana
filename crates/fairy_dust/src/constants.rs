//! Crate-level constants shared by flat top-level modules.

use std::time::Duration;

use bevy::prelude::Color;
use bevy::prelude::KeyCode;
use bevy::prelude::Vec3;
use bevy_diegetic::Pt;
use bevy_diegetic::Px;

// camera home
pub(crate) const HOME_CONTROL: &str = "H Home";
pub(crate) const HOME_DEFAULT_DURATION: Duration = Duration::from_millis(800);
pub(crate) const HOME_DEFAULT_MARGIN: f32 = 0.15;
pub(crate) const HOME_KEY: KeyCode = KeyCode::KeyH;

// camera restart
pub(crate) const RESTART_CAMERA_RESTORE_DURATION: Duration = Duration::from_secs(2);

// cargo restart
pub(crate) const CARGO_BIN: &str = "cargo";
pub(crate) const CARGO_EXAMPLE_FLAG: &str = "--example";
pub(crate) const CARGO_EXAMPLES_DIR: &str = "examples";
pub(crate) const CARGO_RUN_SUBCOMMAND: &str = "run";
pub(crate) const CARGO_TARGET_DIR: &str = "target";

// cascade shadow
// Matches Bevy's non-webgl default. The auto-fit only narrows the cascade
// distances; the count must stay constant after spawn, because changing the
// number of cascades on a live light desyncs `bevy_light`'s per-cascade
// visibility queues and panics.
pub(crate) const CASCADE_COUNT: usize = 4;
pub(crate) const CASCADE_FIRST_FAR_BOUND: f32 = 6.0;
pub(crate) const CASCADE_MAX_DISTANCE: f32 = 18.0;
pub(crate) const CASCADE_MIN_DISTANCE: f32 = 0.1;
// Auto-fit cascade: once the scene's geometry exists, the key light's cascade
// `maximum_distance` is set to (scene bounding-sphere radius * this multiple),
// measured only over the meshes the key light actually shadows (those sharing
// its render layers, so screen-space UI panels are excluded). The multiple
// bakes in the studio camera's framing standoff; 5.0 gives ~18 for the
// canonical ~5-unit grounds and scales up for larger scenes.
pub(crate) const CASCADE_FIT_RADIUS_MULTIPLE: f32 = 5.0;
// Far bound of the first (high-resolution) cascade as a fraction of the fitted
// `maximum_distance`. 0.2 matches the proven 12/60 split.
pub(crate) const CASCADE_FIRST_BOUND_RATIO: f32 = 0.2;
// An orthographic `OrbitCam` parks at a fixed `(near + far) / 2` distance that a
// small scene's radius-based fit can't reach, so the cascade also extends to
// cover the camera. This headroom keeps the scene's far edge inside the last
// cascade rather than exactly on its boundary; it must stay small enough that
// larger scenes (where the radius term already dominates) are left untouched.
pub(crate) const CASCADE_CAMERA_HEADROOM: f32 = 1.2;
// The auto-fit re-runs each frame so the cascade re-adjusts when the projection
// toggles; it rewrites the config only when `maximum_distance` moves by more
// than this, so a steady camera costs nothing.
pub(crate) const CASCADE_REFIT_EPSILON: f32 = 0.01;

// clear color
pub(crate) const CLEAR_COLOR: Color = Color::srgb(0.012, 0.014, 0.018);

// cube defaults
pub(crate) const CUBE_DEFAULT_COLOR: Color = Color::srgb(0.8, 0.7, 0.6);
pub(crate) const CUBE_DEFAULT_SIZE: f32 = 1.0;

/// Canonical example cube color.
pub const EXAMPLE_CUBE_COLOR: Color = CUBE_DEFAULT_COLOR;
/// Canonical example cube edge length in world units.
pub const EXAMPLE_CUBE_SIZE: f32 = CUBE_DEFAULT_SIZE;
/// Canonical cube transform for a cube sitting on the ground plane with extra clearance.
#[must_use]
pub const fn example_cube_on_ground(clearance: f32) -> Vec3 {
    Vec3::new(0.0, EXAMPLE_CUBE_SIZE * 0.5 + clearance, 0.0)
}

// face text
/// Outward offset for a face label so it does not z-fight the cube surface.
pub(crate) const FACE_TEXT_Z_OFFSET: f32 = 0.001;
/// Canonical blue for cube-mounted labels and panels.
pub const CUBE_FACE_PANEL_BLUE: Color = Color::srgb(0.1, 0.35, 1.0);
/// Default time that cube face input labels remain visible after release.
pub const CUBE_FACE_PANEL_RELEASE_HOLD: Duration = Duration::from_millis(300);
/// Default face-label size for simple cube-mounted `WorldText` labels.
pub const CUBE_FACE_LABEL_SIZE: f32 = 0.095;

// fill light
pub(crate) const FILL_LIGHT_ILLUMINANCE: f32 = 1_400.0;
pub(crate) const FILL_LIGHT_POS: Vec3 = Vec3::new(4.5, 4.0, -3.5);

// ground plane defaults
pub(crate) const GROUND_PLANE_ALPHA: f32 = 0.78;
pub(crate) const GROUND_PLANE_DEFAULT_COLOR: Color = Color::srgb(0.125, 0.14, 0.16);
pub(crate) const GROUND_PLANE_DEFAULT_SIZE: f32 = 8.0;
pub(crate) const GROUND_PLANE_METALLIC: f32 = 0.0;
pub(crate) const GROUND_PLANE_REFLECTANCE: f32 = 0.45;
pub(crate) const GROUND_PLANE_ROUGHNESS: f32 = 0.40;

// key light
pub(crate) const KEY_LIGHT_ILLUMINANCE: f32 = 13_500.0;
pub(crate) const KEY_LIGHT_POS: Vec3 = Vec3::new(-3.5, 7.0, 4.8);
pub(crate) const KEY_SHADOW_DEPTH_BIAS: f32 = 0.03;
pub(crate) const KEY_SHADOW_NORMAL_BIAS: f32 = 0.7;

// log filter
/// Default `tracing` filter applied by [`crate::sprinkle_example`].
///
/// Quiets the most common chatty crates (`wgpu`, `naga`) while leaving the
/// rest at `info` so example-side `info!`/`warn!` calls remain visible.
pub const LOG_FILTER: &str = "info,wgpu=error,naga=error,bevy_winit=warn,bevy_render=warn";

// point light
pub(crate) const POINT_LIGHT_COLOR: Color = Color::srgb(0.45, 0.68, 1.0);
pub(crate) const POINT_LIGHT_INTENSITY: f32 = 1_900.0;
pub(crate) const POINT_LIGHT_POS: Vec3 = Vec3::new(-2.0, 1.15, 1.85);
pub(crate) const POINT_LIGHT_RANGE: f32 = 6.0;

// shadow map
pub(crate) const SHADOW_MAP_SIZE: usize = 4096;

// target
pub(crate) const TARGET: Vec3 = Vec3::new(0.0, 0.45, 0.0);

// theme borders
pub(crate) const BORDER: Px = Px(2.0);
pub(crate) const BORDER_ACCENT: Color = Color::srgba(0.15, 0.7, 0.9, 0.5);
pub(crate) const BORDER_DIM: Color = Color::srgba(0.1, 0.4, 0.6, 0.3);
pub(crate) const INNER_BORDER_WIDTH: Px = Px(1.0);

// theme colors
/// Default inner background color for `fairy_dust` screen panels.
///
/// Used by the title bar, description panel, and camera control panel.
/// Exposed publicly so callers tweaking only the alpha can do:
/// `panel.with_background_color(DEFAULT_PANEL_BACKGROUND.with_alpha(0.85))`.
pub const DEFAULT_PANEL_BACKGROUND: Color = Color::srgba(0.02, 0.03, 0.07, 0.80);
pub(crate) const INNER_BACKGROUND: Color = DEFAULT_PANEL_BACKGROUND;
/// Canonical HUD title/header color used by Fairy Dust screen panels.
pub const TITLE_COLOR: Color = Color::srgb(0.9, 0.95, 1.0);

// theme padding
pub(crate) const FRAME_PAD: Px = Px(2.0);
pub(crate) const INNER_PAD: Px = Px(10.0);
const INSET: Px = Px(FRAME_PAD.0 + BORDER.0);

// theme radius
pub(crate) const INNER_RADIUS: Px = Px(RADIUS.0 - INSET.0);
pub(crate) const RADIUS: Px = Px(12.0);

/// Canonical HUD title size. Used by `fairy_dust` panels and re-exported
/// for ad-hoc panels that want to match the built-in look.
pub const TITLE_SIZE: Pt = Pt(14.0);
/// Canonical HUD body / label size. Used by `fairy_dust` panels and re-exported
/// for ad-hoc panels that want to match the built-in look.
pub const LABEL_SIZE: Pt = Pt(11.0);
