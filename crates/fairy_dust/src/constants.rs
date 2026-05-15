//! Crate-level constants shared by flat top-level modules.

use std::time::Duration;

use bevy::prelude::Color;
use bevy::prelude::KeyCode;
use bevy::prelude::Vec3;
use bevy_diegetic::Pt;
use bevy_diegetic::Px;

// ambient
pub(crate) const AMBIENT_BRIGHTNESS: f32 = 95.0;
pub(crate) const AMBIENT_COLOR: Color = Color::srgb(0.55, 0.62, 0.76);

// camera home
pub(crate) const HOME_CONTROL: &str = "H Home";
pub(crate) const HOME_DEFAULT_DURATION: Duration = Duration::from_millis(800);
pub(crate) const HOME_DEFAULT_MARGIN: f32 = 0.15;
pub(crate) const HOME_KEY: KeyCode = KeyCode::KeyH;

// cascade shadow
pub(crate) const CASCADE_FIRST_FAR_BOUND: f32 = 6.0;
pub(crate) const CASCADE_MAX_DISTANCE: f32 = 18.0;
pub(crate) const CASCADE_MIN_DISTANCE: f32 = 0.1;

// clear color
pub(crate) const CLEAR_COLOR: Color = Color::srgb(0.012, 0.014, 0.018);

// cube defaults
pub(crate) const CUBE_DEFAULT_COLOR: Color = Color::srgb(0.8, 0.7, 0.6);
pub(crate) const CUBE_DEFAULT_SIZE: f32 = 1.0;

// face text
/// Outward offset for a face label so it does not z-fight the cube surface.
pub(crate) const FACE_TEXT_Z_OFFSET: f32 = 0.001;

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
pub(crate) const INNER_BG: Color = Color::srgba(0.02, 0.03, 0.07, 0.50);
pub(crate) const TITLE_COLOR: Color = Color::srgb(0.9, 0.95, 1.0);

// theme padding
pub(crate) const FRAME_PAD: Px = Px(2.0);
pub(crate) const INNER_PAD: Px = Px(10.0);
const INSET: Px = Px(FRAME_PAD.0 + BORDER.0);

// theme radius
pub(crate) const INNER_RADIUS: Px = Px(RADIUS.0 - INSET.0);
pub(crate) const RADIUS: Px = Px(12.0);

// theme typography
pub(crate) const TITLE_SIZE: Pt = Pt(14.0);

// trampoline
pub(crate) const TRAMPOLINE_ENV: &str = "FAIRY_DUST_RESTART_TRAMPOLINE";
pub(crate) const TRAMPOLINE_SLEEP: Duration = Duration::from_millis(500);
