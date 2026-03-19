//! Text rendering system — extracts text from layout results and builds glyph meshes.

use std::sync::PoisonError;

use bevy::prelude::*;

use super::glyph_quad::GlyphQuadData;
use super::glyph_quad::build_glyph_mesh;
use super::msdf_material::MsdfTextMaterial;
use crate::layout::RenderCommandKind;
use crate::layout::TextConfig;
use crate::plugin::ComputedDiegeticPanel;
use crate::plugin::DiegeticPanel;
use crate::text::DEFAULT_CANONICAL_SIZE;
use crate::text::FontId;
use crate::text::FontRegistry;
use crate::text::GlyphKey;
use crate::text::MsdfAtlas;

/// Z offset for text layer (above rectangles, below borders).
const TEXT_Z_OFFSET: f32 = 0.001;

/// Marker component for text mesh entities spawned by the renderer.
#[derive(Component)]
struct DiegeticTextMesh;

/// Plugin that adds MSDF text rendering for diegetic panels.
///
/// Registers the [`MsdfTextMaterial`], adds the text extraction system,
/// and sets up rendering.
pub struct TextRenderPlugin;

impl Plugin for TextRenderPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(MaterialPlugin::<MsdfTextMaterial>::default());
        app.add_systems(
            PostUpdate,
            extract_text_meshes.after(crate::plugin::compute_panel_layouts),
        );
    }
}

/// Extracts `RenderCommandKind::Text` entries from computed panels and
/// builds glyph mesh entities with [`MsdfTextMaterial`].
#[allow(clippy::too_many_arguments)]
fn extract_text_meshes(
    panels: Query<(Entity, &DiegeticPanel, &ComputedDiegeticPanel), Changed<ComputedDiegeticPanel>>,
    old_text: Query<(Entity, &ChildOf), With<DiegeticTextMesh>>,
    mut atlas: ResMut<MsdfAtlas>,
    font_registry: Res<FontRegistry>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<MsdfTextMaterial>>,
    mut commands: Commands,
) {
    for (panel_entity, panel, computed) in &panels {
        let Some(result) = &computed.result else {
            continue;
        };

        // Despawn previous text mesh children for this panel.
        for (entity, child_of) in &old_text {
            if child_of.parent() == panel_entity {
                commands.entity(entity).despawn();
            }
        }

        let scale_x = panel.world_width / panel.layout_width;
        let scale_y = panel.world_height / panel.layout_height;
        let half_w = panel.world_width * 0.5;
        let half_h = panel.world_height * 0.5;

        // Get the shared material handle (create once per atlas state).
        let Some(atlas_image) = atlas.image_handle().cloned() else {
            continue;
        };
        #[allow(clippy::cast_possible_truncation)]
        let material_handle = materials.add(MsdfTextMaterial::new(
            LinearRgba::WHITE,
            atlas.sdf_range() as f32,
            atlas.width(),
            atlas.height(),
            atlas_image,
        ));

        for cmd in &result.commands {
            let (text, config) = match &cmd.kind {
                RenderCommandKind::Text { text, config } => (text.as_str(), config),
                _ => continue,
            };

            let quads = shape_text_to_quads(
                text,
                config,
                &cmd.bounds,
                &font_registry,
                &mut atlas,
                scale_x,
                scale_y,
                half_w,
                half_h,
            );

            if quads.is_empty() {
                bevy::log::debug!("text_renderer: no quads for '{text}'");
                continue;
            }
            let min_x = quads.iter().map(|q| q.position[0]).fold(f32::MAX, f32::min);
            let max_x = quads.iter().map(|q| q.position[0] + q.size[0]).fold(f32::MIN, f32::max);
            let extent_w = max_x - min_x;
            let layout_w = cmd.bounds.width * scale_x;
            bevy::log::debug!(
                "text_renderer: '{text}' → {} quads | render_w={extent_w:.3} layout_w={layout_w:.3} ratio={:.2} | first size={:?}",
                quads.len(),
                extent_w / layout_w,
                quads[0].size,
            );

            let mesh = build_glyph_mesh(&quads);
            let mesh_handle = meshes.add(mesh);

            commands.entity(panel_entity).with_child((
                DiegeticTextMesh,
                Mesh3d(mesh_handle),
                MeshMaterial3d(material_handle.clone()),
                Transform::IDENTITY,
            ));
        }
    }
}

/// Shapes text and produces glyph quads in panel-local coordinates.
#[allow(clippy::too_many_arguments)]
fn shape_text_to_quads(
    text: &str,
    config: &TextConfig,
    bounds: &crate::layout::BoundingBox,
    font_registry: &FontRegistry,
    atlas: &mut MsdfAtlas,
    scale_x: f32,
    scale_y: f32,
    half_w: f32,
    half_h: f32,
) -> Vec<GlyphQuadData> {
    let font_cx = font_registry.font_context();
    let mut font_cx = font_cx.lock().unwrap_or_else(PoisonError::into_inner);

    let family_name = font_registry
        .family_name(FontId(config.font_id()))
        .unwrap_or("JetBrains Mono");

    // Use parley to shape the text and get glyph positions.
    let mut layout_cx = parley::LayoutContext::<()>::default();
    let mut layout = parley::Layout::<()>::new();

    let mut builder = layout_cx.ranged_builder(&mut font_cx, text, 1.0, true);
    builder.push_default(parley::style::StyleProperty::FontSize(config.size()));
    builder.push_default(parley::style::StyleProperty::FontStack(
        parley::style::FontStack::Single(parley::style::FontFamily::Named(family_name.into())),
    ));
    // Must match the measurer's line height so baseline positions agree
    // with the layout engine's bounding boxes.
    let line_height = config.effective_line_height();
    builder.push_default(parley::style::StyleProperty::LineHeight(
        parley::style::LineHeight::Absolute(line_height),
    ));
    builder.build_into(&mut layout, text);
    layout.break_all_lines(None);

    // Drop locks before iterating.
    drop(font_cx);
    drop(layout_cx);

    let mut quads = Vec::new();
    let font_data = crate::text::EMBEDDED_FONT;

    for line in layout.lines() {
        bevy::log::debug!(
            "  line metrics: baseline={:.2} ascent={:.2} descent={:.2} line_height={:.2} bounds_y={:.2}",
            line.metrics().baseline,
            line.metrics().ascent,
            line.metrics().descent,
            line.metrics().line_height,
            bounds.y,
        );
        for item in line.items() {
            let parley::layout::PositionedLayoutItem::GlyphRun(run) = item else {
                continue;
            };
            let glyph_run = run.run();

            let mut advance_x = 0.0_f32;
            for cluster in glyph_run.clusters() {
                for glyph in cluster.glyphs() {
                    #[allow(clippy::cast_possible_truncation)]
                    let key = GlyphKey {
                        font_id:     config.font_id(),
                        glyph_index: glyph.id as u16,
                    };

                    let Some(metrics) = atlas.get_or_insert(key, font_data) else {
                        advance_x += glyph.advance;
                        continue;
                    };

                    // Glyph position in layout coordinates (Y-down).
                    // run.offset() = x start of run, advance_x = accumulated advance,
                    // glyph.x/y = fine offset within cluster.
                    let glyph_x = bounds.x + run.offset() + advance_x + glyph.x;
                    let glyph_y = bounds.y + run.baseline() - glyph.y;

                    // Scale from canonical atlas pixels to layout units.
                    #[allow(clippy::cast_precision_loss)]
                    let em_scale = config.size() / DEFAULT_CANONICAL_SIZE as f32;

                    // Glyph quad size in layout units.
                    #[allow(clippy::cast_precision_loss)]
                    let quad_w = metrics.pixel_width as f32 * em_scale;
                    #[allow(clippy::cast_precision_loss)]
                    let quad_h = metrics.pixel_height as f32 * em_scale;

                    // Quad top-left in layout coordinates:
                    // X: glyph origin + bearing (shifts left for SDF padding)
                    // Y: baseline - bearing_y (shifts up from baseline to quad top)
                    let quad_layout_x = metrics.bearing_x.mul_add(config.size(), glyph_x);
                    let quad_layout_y = (-metrics.bearing_y).mul_add(config.size(), glyph_y);

                    // Convert layout coordinates to panel-local (center origin, Y-up).
                    let local_x = quad_layout_x.mul_add(scale_x, -half_w);
                    let local_y = (-quad_layout_y).mul_add(scale_y, half_h);

                    let quad_world_w = quad_w * scale_x;
                    let quad_world_h = quad_h * scale_y;

                    quads.push(GlyphQuadData {
                        position: [local_x, local_y, TEXT_Z_OFFSET],
                        size:     [quad_world_w, quad_world_h],
                        uv_rect:  metrics.uv_rect,
                    });

                    advance_x += glyph.advance;
                }
            }
        }
    }

    quads
}
