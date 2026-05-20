//! Isolated Slug feasibility example.
//!
//! This intentionally does not use the production text renderer. It loads
//! embedded fonts, converts supported glyph outlines to quadratic curves,
//! packs them into banded curve data, and renders the fixtures through the
//! isolated Slug shader path.

use bevy::prelude::*;
use bevy::render::storage::ShaderStorageBuffer;
use bevy_diegetic::Anchor;
use bevy_diegetic::FontId;
use bevy_diegetic::GlyphShadowMode;
use bevy_diegetic::WorldText;
use bevy_diegetic::WorldTextStyle;
use bevy_diegetic::slug_text_spike::DEFAULT_BAND_COUNT;
use bevy_diegetic::slug_text_spike::FIXTURE_TEXT;
use bevy_diegetic::slug_text_spike::SlugFontKey;
use bevy_diegetic::slug_text_spike::SlugGlyphCache;
use bevy_diegetic::slug_text_spike::SlugGlyphInstance;
use bevy_diegetic::slug_text_spike::SlugGlyphKey;
use bevy_diegetic::slug_text_spike::SlugOutlineError;
use bevy_diegetic::slug_text_spike::SlugPackedGlyph;
use bevy_diegetic::slug_text_spike::SlugTextMaterial;
use bevy_diegetic::slug_text_spike::SlugTextMaterialInput;
use bevy_diegetic::slug_text_spike::SlugTextRun;
use bevy_diegetic::slug_text_spike::SlugTextSpikePlugin;
use bevy_diegetic::slug_text_spike::build_packed_glyph;
use bevy_diegetic::slug_text_spike::load_glyph;
use bevy_diegetic::slug_text_spike::slug_text_material;
use bevy_kana::ToU16;
use bevy_lagrange::OrbitCamPreset;
use fairy_dust::TitleBar;
use parley::fontique::Blob;
use parley::fontique::FontInfoOverride;
use parley::layout::PositionedLayoutItem;
use parley::style::FontFamily;
use parley::style::StyleProperty;
use ttf_parser::Face;

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

#[derive(Clone, Copy, Debug)]
struct ShapedPreviewGlyph {
    character: char,
    glyph_id:  u16,
    origin:    Vec2,
    advance:   f32,
}

#[derive(Clone, Debug)]
struct PreviewText {
    run:            SlugTextRun,
    glyph_cache:    SlugGlyphCache,
    baseline:       f32,
    reference_size: f32,
}

fn main() {
    // `bevy_diegetic::DiegeticUiPlugin` is registered automatically by
    // `fairy_dust::sprinkle_example`.
    fairy_dust::sprinkle_example()
        .with_brp_extras()
        .with_save_window_position()
        .with_studio_lighting()
        .with_ground_plane()
        .with_orbit_cam(|_| {}, OrbitCamPreset::BlenderLike)
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
            spawn_glyphs(
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

fn spawn_world_text_reference(commands: &mut Commands, preview: &PreviewText) {
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
            baseline_y + preview.baseline * FONT_SCALE,
            WORLD_TEXT_REFERENCE_Z,
        ),
    ));
}

fn load_preview_text() -> Result<PreviewText, SlugOutlineError> {
    let shaped_text = shape_preview_glyphs()?;
    let mut glyph_cache = SlugGlyphCache::default();
    let mut glyphs = Vec::with_capacity(shaped_text.glyphs.len());
    for glyph in shaped_text.glyphs {
        let key = SlugGlyphKey::new(LATIN_FONT_KEY, glyph.glyph_id);
        let packed_glyph = glyph_cache.get_or_insert_packed(
            key,
            LATIN_FONT_DATA,
            glyph.character,
            DEFAULT_BAND_COUNT,
        )?;
        glyphs.push(SlugGlyphInstance::new(
            key,
            glyph.origin,
            glyph.advance,
            packed_glyph.bounds(),
        ));
    }
    Ok(PreviewText {
        run: SlugTextRun::new(glyphs),
        glyph_cache,
        baseline: shaped_text.baseline,
        reference_size: shaped_text.reference_size,
    })
}

#[derive(Clone, Debug)]
struct ShapedPreviewText {
    glyphs:         Vec<ShapedPreviewGlyph>,
    baseline:       f32,
    reference_size: f32,
}

fn shape_preview_glyphs() -> Result<ShapedPreviewText, SlugOutlineError> {
    let face = Face::parse(LATIN_FONT_DATA, 0).map_err(|_| SlugOutlineError::InvalidFont)?;
    let shape_size = f32::from(face.units_per_em());

    let mut font_context = parley::FontContext::default();
    font_context.collection.register_fonts(
        Blob::from(LATIN_FONT_DATA.to_vec()),
        Some(FontInfoOverride {
            family_name: Some(LATIN_FONT_FAMILY),
            ..default()
        }),
    );
    let mut layout_context = parley::LayoutContext::<()>::default();
    let mut layout = parley::Layout::<()>::new();

    let mut builder = layout_context.ranged_builder(&mut font_context, FIXTURE_TEXT, 1.0, true);
    builder.push_default(StyleProperty::FontSize(shape_size));
    builder.push_default(StyleProperty::FontFamily(FontFamily::named(
        LATIN_FONT_FAMILY,
    )));
    builder.build_into(&mut layout, FIXTURE_TEXT);
    layout.break_all_lines(None);

    let mut characters = FIXTURE_TEXT.chars();
    let mut shaped_glyphs = Vec::new();
    let mut baseline = 0.0;
    for line in layout.lines() {
        baseline = line.metrics().baseline;
        for item in line.items() {
            let PositionedLayoutItem::GlyphRun(run) = item else {
                continue;
            };
            let mut advance_x = 0.0_f32;
            for cluster in run.run().clusters() {
                for glyph in cluster.glyphs() {
                    let Some(character) = characters.next() else {
                        continue;
                    };
                    shaped_glyphs.push(ShapedPreviewGlyph {
                        character,
                        glyph_id: glyph.id.to_u16(),
                        origin: Vec2::new(run.offset() + advance_x + glyph.x, glyph.y),
                        advance: glyph.advance,
                    });
                    advance_x += glyph.advance;
                }
            }
        }
    }
    Ok(ShapedPreviewText {
        glyphs: shaped_glyphs,
        baseline,
        reference_size: shape_size * FONT_SCALE,
    })
}

fn log_preview_metrics(preview: &PreviewText) {
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

fn spawn_glyphs(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    slug_materials: &mut Assets<SlugTextMaterial>,
    storage_buffers: &mut Assets<ShaderStorageBuffer>,
    preview: &PreviewText,
) {
    let total_width = text_width(&preview.run);
    let run_origin_x = total_width * -0.5;
    for glyph in preview.run.glyphs() {
        let Some(packed_glyph) = preview.glyph_cache.get(glyph.key()) else {
            warn!(
                "slug glyph cache missing glyph id {}",
                glyph.key().glyph_id()
            );
            continue;
        };
        spawn_glyph(
            commands,
            meshes,
            slug_materials,
            storage_buffers,
            packed_glyph,
            Vec2::new(run_origin_x, 0.0) + glyph.origin() * FONT_SCALE,
        );
    }
}

fn spawn_glyph(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    slug_materials: &mut Assets<SlugTextMaterial>,
    storage_buffers: &mut Assets<ShaderStorageBuffer>,
    packed_glyph: &SlugPackedGlyph,
    origin: Vec2,
) {
    spawn_filled_glyph(
        commands,
        meshes,
        slug_materials,
        storage_buffers,
        packed_glyph,
        origin,
    );
}

fn spawn_filled_glyph(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    slug_materials: &mut Assets<SlugTextMaterial>,
    storage_buffers: &mut Assets<ShaderStorageBuffer>,
    packed_glyph: &SlugPackedGlyph,
    origin: Vec2,
) {
    let glyph = packed_glyph.glyph();
    let curve_buffer =
        storage_buffers.add(ShaderStorageBuffer::from(packed_glyph.curves().to_vec()));
    let band_buffer = storage_buffers.add(ShaderStorageBuffer::from(packed_glyph.bands().to_vec()));
    let material = slug_materials.add(slug_text_material(SlugTextMaterialInput {
        base:       StandardMaterial::default(),
        bounds:     packed_glyph.bounds(),
        fill_color: SLUG_FILL_COLOR,
        curves:     curve_buffer,
        bands:      band_buffer,
        band_count: packed_glyph.bands().len(),
    }));
    let bounds_width = glyph.bounds.width() * FONT_SCALE;
    let bounds_height = glyph.bounds.height() * FONT_SCALE;
    commands.spawn((
        Name::new(format!("SlugGlyphFill {}", glyph.character)),
        SlugGlyphPreview,
        Mesh3d(meshes.add(Rectangle::new(bounds_width, bounds_height))),
        MeshMaterial3d(material),
        Transform::from_xyz(
            origin.x + glyph.bounds.min.x.mul_add(FONT_SCALE, bounds_width * 0.5),
            PREVIEW_ELEVATION
                + GLYPH_BASELINE_Y
                + origin.y
                + glyph.bounds.min.y.mul_add(FONT_SCALE, bounds_height * 0.5),
            0.0,
        ),
    ));
}

fn text_width(run: &SlugTextRun) -> f32 { run.advance_width() * FONT_SCALE }
