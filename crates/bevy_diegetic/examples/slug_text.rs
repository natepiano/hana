//! Isolated Slug feasibility example.
//!
//! This intentionally does not use the production text renderer. It loads
//! embedded fonts, converts supported glyph outlines to quadratic curves,
//! packs them into banded curve data, and renders the fixtures through the
//! isolated Slug shader path.

use bevy::prelude::*;
use bevy::render::storage::ShaderStorageBuffer;
use bevy_diegetic::Anchor;
use bevy_diegetic::DEFAULT_BAND_COUNT;
use bevy_diegetic::FIXTURE_TEXT;
use bevy_diegetic::FontId;
use bevy_diegetic::GlyphShadowMode;
use bevy_diegetic::SlugBuiltTextRun;
use bevy_diegetic::SlugFontKey;
use bevy_diegetic::SlugOutlineError;
use bevy_diegetic::SlugPackedGlyph;
use bevy_diegetic::SlugRenderMode;
use bevy_diegetic::SlugTextMaterial;
use bevy_diegetic::SlugTextMaterialInput;
use bevy_diegetic::SlugTextRun;
use bevy_diegetic::SlugTextSpikePlugin;
use bevy_diegetic::WorldText;
use bevy_diegetic::WorldTextStyle;
use bevy_diegetic::build_packed_glyph;
use bevy_diegetic::build_slug_run_render_data;
use bevy_diegetic::build_slug_text_run;
use bevy_diegetic::load_glyph;
use bevy_diegetic::slug_text_material;
use bevy_lagrange::OrbitCamInputMode;
use bevy_lagrange::OrbitCamPreset;
use fairy_dust::TitleBar;

const LATIN_FONT_DATA: &[u8] = include_bytes!("../assets/fonts/JetBrainsMono-Regular.ttf");
const LATIN_FONT_FAMILY: &str = "JetBrains Mono";
const LATIN_FONT_KEY: SlugFontKey = SlugFontKey::new(0);
const CJK_FONT_DATA: &[u8] = include_bytes!("../assets/fonts/NotoSansCJKsc-Regular.otf");
const CJK_SAMPLE_CHAR: char = '漢';
const FONT_SCALE: f32 = 0.0015;
const GLYPH_BASELINE_Y: f32 = -0.6;
const PREVIEW_ELEVATION: f32 = 1.2;
const WORLD_TEXT_REFERENCE_Z: f32 = -0.0001;
const SLUG_FILL_COLOR: Color = Color::srgba(1.0, 0.38, 0.20, 1.0);
const WORLD_TEXT_REFERENCE_COLOR: Color = Color::WHITE;
const HOME_FOCUS: Vec3 = Vec3::new(0.0, PREVIEW_ELEVATION, 0.0);
const HOME_FRAME_SIZE: f32 = 5.2;
const HOME_PITCH: f32 = 0.0;
const HOME_YAW: f32 = 0.0;
const TITLE_CONTROL: &str = "Scroll Zoom";

#[derive(Component)]
struct SlugGlyphPreview;

fn main() {
    // `bevy_diegetic::DiegeticUiPlugin` is registered automatically by
    // `fairy_dust::sprinkle_example`.
    fairy_dust::sprinkle_example()
        .with_brp_extras()
        .with_save_window_position()
        .with_studio_lighting()
        .with_ground_plane()
        .with_orbit_cam(
            |_| {},
            OrbitCamInputMode::Preset(OrbitCamPreset::BlenderLike),
        )
        .with_stable_transparency()
        .with_camera_home(
            Transform::from_translation(HOME_FOCUS).with_scale(Vec3::splat(HOME_FRAME_SIZE)),
        )
        .pitch(HOME_PITCH)
        .yaw(HOME_YAW)
        .with_title_bar(
            TitleBar::new()
                .with_anchor(Anchor::TopLeft)
                .control(TITLE_CONTROL),
        )
        .with_camera_control_panel()
        .add_plugins(SlugTextSpikePlugin)
        .add_systems(Startup, setup)
        .run();
}

fn setup(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut slug_materials: ResMut<Assets<SlugTextMaterial>>,
    mut storage_buffers: ResMut<Assets<ShaderStorageBuffer>>,
) {
    match load_preview_text() {
        Ok(preview) => {
            log_preview_metrics(&preview);
            log_cjk_probe();
            spawn_world_text_reference(&mut commands, &preview);
            spawn_slug_text_run(
                &mut commands,
                &mut meshes,
                &mut slug_materials,
                &mut storage_buffers,
                &preview,
            );
        },
        Err(err) => {
            error!("slug_text feasibility example failed: {err}");
        },
    }
}

fn spawn_world_text_reference(commands: &mut Commands, preview: &SlugBuiltTextRun) {
    let baseline_y = PREVIEW_ELEVATION + GLYPH_BASELINE_Y;
    commands.spawn((
        Name::new("WorldText Typography Reference"),
        WorldText::new(FIXTURE_TEXT),
        WorldTextStyle::new(preview.reference_size)
            .with_font(FontId::MONOSPACE.0)
            .with_color(WORLD_TEXT_REFERENCE_COLOR)
            .with_anchor(Anchor::TopLeft)
            .with_shadow_mode(GlyphShadowMode::None),
        Transform::from_xyz(
            text_width(&preview.run) * -0.5,
            preview.baseline.mul_add(FONT_SCALE, baseline_y),
            WORLD_TEXT_REFERENCE_Z,
        ),
    ));
}

fn load_preview_text() -> Result<SlugBuiltTextRun, SlugOutlineError> {
    build_slug_text_run(
        FIXTURE_TEXT,
        LATIN_FONT_DATA,
        LATIN_FONT_KEY,
        LATIN_FONT_FAMILY,
        FONT_SCALE,
        DEFAULT_BAND_COUNT,
    )
}

fn log_preview_metrics(preview: &SlugBuiltTextRun) {
    info!(
        "parley preview run: glyph_instances={}, unique_packed_glyphs={}, \
        advance_width={}, bounds=({}, {})..({}, {})",
        preview.run.glyphs().len(),
        preview.glyph_cache.len(),
        preview.run.advance_width(),
        preview.run.bounds().min.x,
        preview.run.bounds().min.y,
        preview.run.bounds().max.x,
        preview.run.bounds().max.y
    );
    for glyph in preview.run.glyphs() {
        if let Some(packed_glyph) = preview.glyph_cache.get(glyph.key()) {
            log_glyph_metrics("parley preview", packed_glyph);
        }
    }
}

fn log_cjk_probe() {
    match load_glyph(CJK_FONT_DATA, CJK_SAMPLE_CHAR) {
        Ok(glyph) => {
            let packed_glyph = build_packed_glyph(glyph, DEFAULT_BAND_COUNT);
            log_glyph_metrics("cjk probe", &packed_glyph);
        },
        Err(SlugOutlineError::CubicOutline {
            character,
            cubic_segments,
        }) => {
            warn!(
                "cjk probe rejected '{character}': {cubic_segments} cubic outline segments; \
                cubic conversion is intentionally deferred"
            );
        },
        Err(err) => warn!("cjk probe failed: {err}"),
    }
}

fn log_glyph_metrics(label: &str, packed_glyph: &SlugPackedGlyph) {
    let glyph = packed_glyph.glyph();
    info!(
        "{label} '{}': glyph_id={}, contours={}, outline_segments={}, bands={}, \
        packed_curve_records={}, curve_bytes={}, band_bytes={}",
        glyph.character,
        glyph.glyph_id,
        glyph.contour_count(),
        packed_glyph.outline_segments(),
        packed_glyph.bands().len(),
        packed_glyph.duplicated_curves(),
        packed_glyph.curve_bytes(),
        packed_glyph.band_bytes()
    );
}

fn spawn_slug_text_run(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    slug_materials: &mut Assets<SlugTextMaterial>,
    storage_buffers: &mut Assets<ShaderStorageBuffer>,
    preview: &SlugBuiltTextRun,
) {
    let render_data = match build_slug_run_render_data(preview, FONT_SCALE) {
        Ok(render_data) => render_data,
        Err(err) => {
            error!("failed to build run-level Slug render data: {err}");
            return;
        },
    };
    let curve_buffer = storage_buffers.add(ShaderStorageBuffer::from(render_data.curves));
    let band_buffer = storage_buffers.add(ShaderStorageBuffer::from(render_data.bands));
    let glyph_buffer = storage_buffers.add(ShaderStorageBuffer::from(render_data.glyphs));
    let material = slug_materials.add(slug_text_material(SlugTextMaterialInput {
        base:        StandardMaterial::default(),
        fill_color:  SLUG_FILL_COLOR,
        render_mode: SlugRenderMode::Text,
        curves:      curve_buffer,
        bands:       band_buffer,
        glyphs:      glyph_buffer,
    }));
    let run_origin_x = text_width(&preview.run) * -0.5;
    commands.spawn((
        Name::new("SlugTextRun Typography"),
        SlugGlyphPreview,
        Mesh3d(meshes.add(render_data.mesh)),
        MeshMaterial3d(material),
        Transform::from_xyz(run_origin_x, PREVIEW_ELEVATION + GLYPH_BASELINE_Y, 0.0),
    ));
}

fn text_width(run: &SlugTextRun) -> f32 { run.advance_width() * FONT_SCALE }
