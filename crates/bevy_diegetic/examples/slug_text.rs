//! Isolated Slug feasibility example.
//!
//! This intentionally does not use the production text renderer. It loads
//! embedded fonts, converts supported glyph outlines to quadratic curves,
//! packs them into banded curve data, and renders the fixtures through the
//! isolated Slug shader path.

use std::time::Instant;

use bevy::prelude::*;
use bevy::render::storage::ShaderStorageBuffer;
use bevy_diegetic::Anchor;
use bevy_diegetic::DEFAULT_BAND_COUNT;
use bevy_diegetic::FIXTURE_TEXT;
use bevy_diegetic::SlugBackend;
use bevy_diegetic::SlugBackendCompleted;
use bevy_diegetic::SlugBuiltTextRun;
use bevy_diegetic::SlugFontKey;
use bevy_diegetic::SlugOutlineError;
use bevy_diegetic::SlugPackedGlyph;
use bevy_diegetic::SlugRenderMode;
use bevy_diegetic::SlugTextMaterial;
use bevy_diegetic::SlugTextMaterialInput;
use bevy_diegetic::SlugTextRequest;
use bevy_diegetic::SlugTextRun;
use bevy_diegetic::build_packed_glyph;
use bevy_diegetic::build_slug_run_render_data;
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
const DISPLAY_SIZE: f32 = 0.48;
const DISPLAY_Y: f32 = 0.5;
const DISPLAY_Z: f32 = 2.0;
const JETBRAINS_UNITS_PER_EM: f32 = 1000.0;
const FONT_SCALE: f32 = DISPLAY_SIZE / JETBRAINS_UNITS_PER_EM;
const SLUG_FILL_COLOR: Color = Color::srgba(1.0, 0.38, 0.20, 1.0);
const GROUND_SIZE: f32 = 5.4;
const GROUND_DEPTH_SCALE: f32 = 0.7;
const GROUND_CENTER_Z: f32 = GROUND_SIZE * 0.5 * (1.0 - GROUND_DEPTH_SCALE);
const GROUND_COLOR: Color = Color::srgb(0.08, 0.08, 0.08);
const HOME_FOCUS: Vec3 = Vec3::new(-0.001, 0.461, 2.002);
const HOME_RADIUS: f32 = 2.84;
const HOME_PITCH: f32 = 0.055;
const HOME_YAW: f32 = 0.0;
const LIGHT_AIM: Vec3 = Vec3::new(0.0, DISPLAY_Y, DISPLAY_Z);
const KEY_LIGHT_POS: Vec3 = Vec3::new(0.0, 5.0, DISPLAY_Z + 12.0);
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
        .aim_at(LIGHT_AIM)
        .key_light_pos(KEY_LIGHT_POS)
        .with_ground_plane()
        .size(GROUND_SIZE)
        .transform(
            Transform::from_xyz(0.0, 0.0, GROUND_CENTER_Z).with_scale(Vec3::new(
                1.0,
                1.0,
                GROUND_DEPTH_SCALE,
            )),
        )
        .color(GROUND_COLOR)
        .with_orbit_cam(
            |_| {},
            OrbitCamInputMode::Preset(OrbitCamPreset::BlenderLike),
        )
        .with_stable_transparency()
        .with_camera_home(
            Transform::from_translation(HOME_FOCUS).with_scale(Vec3::splat(HOME_RADIUS * 2.0)),
        )
        .pitch(HOME_PITCH)
        .yaw(HOME_YAW)
        .with_title_bar(
            TitleBar::new()
                .with_anchor(Anchor::TopLeft)
                .control(TITLE_CONTROL),
        )
        .with_camera_control_panel()
        .add_systems(Startup, setup)
        .run();
}

fn setup(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut slug_materials: ResMut<Assets<SlugTextMaterial>>,
    mut storage_buffers: ResMut<Assets<ShaderStorageBuffer>>,
    mut slug_backend: ResMut<SlugBackend>,
) {
    let prepare_start = Instant::now();
    match load_preview_text(&mut slug_backend) {
        Ok(preview) => {
            let prepare_ms = prepare_start.elapsed().as_secs_f32() * 1000.0;
            if let Some(completion) = slug_backend.last_completion() {
                commands.trigger(completion);
                log_slug_backend_completion(completion);
            }
            log_preview_metrics(&preview, &slug_backend, prepare_ms);
            log_cjk_probe(&mut slug_backend);
            spawn_slug_text_run(
                &mut commands,
                &mut meshes,
                &mut slug_materials,
                &mut storage_buffers,
                &preview,
                &slug_backend,
            );
        },
        Err(err) => {
            slug_backend.record_failure();
            error!("slug_text feasibility example failed: {err}");
        },
    }
}

fn load_preview_text(slug_backend: &mut SlugBackend) -> Result<SlugBuiltTextRun, SlugOutlineError> {
    let prepared = slug_backend.prepare_text_run(SlugTextRequest::new(
        FIXTURE_TEXT,
        LATIN_FONT_DATA,
        LATIN_FONT_KEY,
        LATIN_FONT_FAMILY,
        FONT_SCALE,
    ))?;
    Ok(prepared.run)
}

fn log_preview_metrics(preview: &SlugBuiltTextRun, slug_backend: &SlugBackend, prepare_ms: f32) {
    info!(
        "parley preview run: glyph_instances={}, unique_packed_glyphs={}, \
        prepare_ms={prepare_ms:.3}, advance_width={}, bounds=({}, {})..({}, {})",
        preview.run.glyphs().len(),
        slug_backend.glyph_cache().len(),
        preview.run.advance_width(),
        preview.run.bounds().min.x,
        preview.run.bounds().min.y,
        preview.run.bounds().max.x,
        preview.run.bounds().max.y
    );
    for glyph in preview.run.glyphs() {
        if let Some(packed_glyph) = slug_backend.glyph_cache().get(glyph.key()) {
            log_glyph_metrics("parley preview", packed_glyph);
        }
    }
}

fn log_slug_backend_completion(completion: SlugBackendCompleted) {
    info!(
        "slug backend completed generation {} with {} packed glyphs",
        completion.generation, completion.packed_glyphs
    );
}

fn log_cjk_probe(slug_backend: &mut SlugBackend) {
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
        Err(err) => {
            slug_backend.record_failure();
            warn!("cjk probe failed: {err}");
        },
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
    slug_backend: &SlugBackend,
) {
    let render_start = Instant::now();
    let render_data =
        match build_slug_run_render_data(preview, slug_backend.glyph_cache(), FONT_SCALE) {
            Ok(render_data) => render_data,
            Err(err) => {
                error!("failed to build run-level Slug render data: {err}");
                return;
            },
        };
    let render_ms = render_start.elapsed().as_secs_f32() * 1000.0;
    let profile = render_data.profile();
    info!(
        "slug run storage: render_ms={render_ms:.3}, glyph_instances={}, unique_glyphs={}, \
        vertices={}, indices={}, curve_records={}, band_records={}, storage_bytes={}",
        profile.glyph_instances,
        profile.unique_glyphs,
        profile.mesh_vertices,
        profile.mesh_indices,
        profile.curve_records,
        profile.band_records,
        profile.storage_bytes()
    );
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
    let transform = Transform::from_xyz(run_origin_x, DISPLAY_Y, DISPLAY_Z);
    commands.spawn((
        Name::new("SlugTextRun Typography"),
        SlugGlyphPreview,
        Mesh3d(meshes.add(render_data.mesh)),
        MeshMaterial3d(material),
        transform,
    ));
}

fn text_width(run: &SlugTextRun) -> f32 { run.advance_width() * FONT_SCALE }
