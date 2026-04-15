// SDF Panel Fragment Shader — MaterialExtension for StandardMaterial
//
// Renders rounded rectangles with optional borders using signed distance
// fields. Produces pixel-perfect edges at any zoom level with smooth
// anti-aliasing, independent of MSAA.
//
// The quad mesh covers the element's bounding box. The shader computes
// the SDF for a rounded rectangle and uses it for:
//   - Background fill alpha (inside the shape)
//   - Border alpha (between inner and outer edges)
//   - Smooth anti-aliased transitions at all edges
//
// Extends StandardMaterial's PBR pipeline: lighting, shadows, and
// double-sided normals come from the base material.

#import bevy_pbr::{
    pbr_fragment::pbr_input_from_standard_material,
    pbr_functions::alpha_discard,
    pbr_types::STANDARD_MATERIAL_FLAGS_UNLIT_BIT,
}

#ifdef PREPASS_PIPELINE
#import bevy_pbr::prepass_io::VertexOutput
#else
#import bevy_pbr::{
    forward_io::{VertexOutput, FragmentOutput},
    pbr_functions::{apply_pbr_lighting, main_pass_post_lighting_processing},
}
#endif

#ifdef OIT_ENABLED
#import bevy_core_pipeline::oit::oit_draw
#import bevy_pbr::pbr_types::{
    STANDARD_MATERIAL_FLAGS_ALPHA_MODE_RESERVED_BITS,
    STANDARD_MATERIAL_FLAGS_ALPHA_MODE_OPAQUE,
}
#endif

struct SdfPanelUniform {
    /// Half-size of the SDF shape in world units (width/2, height/2).
    half_size:      vec2<f32>,
    /// Half-size of the mesh quad (includes AA padding beyond shape).
    mesh_half_size: vec2<f32>,
    /// Per-corner radii in world units: [TL, TR, BR, BL].
    corner_radii:   vec4<f32>,
    /// Border widths in world units: [top, right, bottom, left].
    border_widths: vec4<f32>,
    /// Border color in linear RGBA.
    border_color:  vec4<f32>,
    /// Alpha of the fill/base color. Lets the prepass distinguish
    /// filled panels from border-only panels.
    fill_alpha:    f32,
    /// Clip rect in local quad space: [left, bottom, right, top].
    /// Fragments outside are discarded.
    clip_rect:         vec4<f32>,
    /// Depth offset for OIT fragment ordering (reverse-Z: positive = closer).
    oit_depth_offset:  f32,
}

@group(#{MATERIAL_BIND_GROUP}) @binding(100) var<uniform> sdf: SdfPanelUniform;

/// Signed distance to a rounded rectangle centered at the origin.
///
/// `p` — fragment position relative to rectangle center.
/// `half_size` — half-width and half-height.
/// `radii` — corner radii as vec4(TL, TR, BR, BL).
fn sd_rounded_box(p: vec2<f32>, half_size: vec2<f32>, radii: vec4<f32>) -> f32 {
    // Select the radius for this quadrant.
    let r = select(radii.xw, radii.yz, p.x > 0.0);
    let radius = select(r.x, r.y, p.y > 0.0);
    let q = abs(p) - half_size + radius;
    return min(max(q.x, q.y), 0.0) + length(max(q, vec2(0.0))) - radius;
}

/// Computes the effective inner half-size after subtracting border widths.
fn inner_half_size(border_widths: vec4<f32>) -> vec2<f32> {
    return vec2<f32>(
        sdf.half_size.x - 0.5 * (border_widths.y + border_widths.w),
        sdf.half_size.y - 0.5 * (border_widths.x + border_widths.z),
    );
}

/// Computes the offset from center due to asymmetric border widths.
fn border_center_offset(border_widths: vec4<f32>) -> vec2<f32> {
    return vec2<f32>(
        0.5 * (border_widths.w - border_widths.y),
        0.5 * (border_widths.x - border_widths.z),
    );
}

/// Shrinks corner radii by the minimum border width on adjacent sides.
fn inner_corner_radii(border_widths: vec4<f32>) -> vec4<f32> {
    return max(
        vec4(0.0),
        vec4<f32>(
            sdf.corner_radii.x - min(border_widths.x, border_widths.w), // TL
            sdf.corner_radii.y - min(border_widths.x, border_widths.y), // TR
            sdf.corner_radii.z - min(border_widths.z, border_widths.y), // BR
            sdf.corner_radii.w - min(border_widths.z, border_widths.w), // BL
        ),
    );
}

/// Anti-aliasing half-width from the screen-space rate of change of a
/// distance field value. Using `fwidth(dist)` directly accounts for
/// perspective foreshortening at extreme viewing angles.
fn aa_width(dist: f32) -> f32 {
    return fwidth(dist) * 0.75;
}

// ── Prepass ─────────────────────────────────────────────────────────

#ifdef PREPASS_PIPELINE
@fragment
fn fragment(in: VertexOutput, @builtin(front_facing) is_front: bool) {
    // Map UV (0..1) to local coordinates centered on the quad.
    let local = (in.uv - 0.5) * 2.0 * sdf.mesh_half_size;

    // Clip to parent scissor rect.
    if local.x < sdf.clip_rect.x || local.x > sdf.clip_rect.z
        || local.y < sdf.clip_rect.y || local.y > sdf.clip_rect.w {
        discard;
    }

    let dist = sd_rounded_box(local, sdf.half_size, sdf.corner_radii);

    // Discard fragments outside the rounded shape.
    if dist > 0.0 {
        discard;
    }

    let has_border = sdf.border_widths.x > 0.0
        || sdf.border_widths.y > 0.0
        || sdf.border_widths.z > 0.0
        || sdf.border_widths.w > 0.0;
    let has_fill = sdf.fill_alpha > 0.001;

    // Border-only panels should cast only the visible ring, not the
    // transparent interior.
    if has_border && !has_fill {
        // Keep border-only casters readable in the shadow map by giving
        // each side at least a 1px screen-space footprint in the prepass.
        let pixel_size = vec2<f32>(fwidth(local.x), fwidth(local.y));
        let shadow_border_widths = max(
            sdf.border_widths,
            vec4<f32>(pixel_size.y, pixel_size.x, pixel_size.y, pixel_size.x),
        );
        let inner_hs = inner_half_size(shadow_border_widths);
        let inner_offset = border_center_offset(shadow_border_widths);
        let inner_radii = inner_corner_radii(shadow_border_widths);
        let inner_dist = sd_rounded_box(local - inner_offset, max(inner_hs, vec2(0.0)), inner_radii);
        if inner_dist <= 0.0 {
            discard;
        }
    }
}
#else

// ── Main pass ───────────────────────────────────────────────────────

@fragment
fn fragment(
    in: VertexOutput,
    @builtin(front_facing) is_front: bool,
) -> FragmentOutput {
    // Map UV (0..1) to local coordinates centered on the quad.
    let local = (in.uv - 0.5) * 2.0 * sdf.mesh_half_size;

    // Clip to parent scissor rect.
    if local.x < sdf.clip_rect.x || local.x > sdf.clip_rect.z
        || local.y < sdf.clip_rect.y || local.y > sdf.clip_rect.w {
        discard;
    }

    // ── Sub-pixel coverage ───────────────────────────────────────────
    // When the element is thinner than a pixel in either axis, inflate
    // the SDF shape to 1 pixel minimum and scale alpha proportionally.
    // A 0.3px line renders as a 1px line at 30% alpha — always visible,
    // always consistent, physically correct when zoomed in.
    let pixel_size = vec2<f32>(fwidth(local.x), fwidth(local.y));
    let min_half = pixel_size * 0.5;
    var effective_half_size = sdf.half_size;
    var coverage_scale = 1.0;
    if sdf.half_size.x < min_half.x && sdf.half_size.x > 0.0 {
        coverage_scale *= sdf.half_size.x / min_half.x;
        effective_half_size.x = min_half.x;
    }
    if sdf.half_size.y < min_half.y && sdf.half_size.y > 0.0 {
        coverage_scale *= sdf.half_size.y / min_half.y;
        effective_half_size.y = min_half.y;
    }

    // Outer shape distance (using potentially inflated half-size).
    let outer_dist = sd_rounded_box(local, effective_half_size, sdf.corner_radii);

    // Base AA width from screen-space derivatives of the SDF.
    let base_aa = aa_width(outer_dist);

    // Outer shape alpha — exterior-only falloff with doubled ramp width.
    // Interior pixels (dist ≤ 0) are fully opaque so adjacent quads
    // sharing an edge don't double-blend and produce visible seams.
    // The 2× ramp compensates for the one-sided shift, preserving the
    // same visual AA band width as a centered smoothstep.
    let outer_alpha = (1.0 - smoothstep(0.0, 2.0 * base_aa, outer_dist)) * coverage_scale;

    // Discard fully outside fragments.
    if outer_alpha < 0.001 {
        discard;
    }

    // Determine if we have a border.
    let has_border = sdf.border_widths.x > 0.0
        || sdf.border_widths.y > 0.0
        || sdf.border_widths.z > 0.0
        || sdf.border_widths.w > 0.0;

    // Inner shape distance (inside the border).
    let inner_hs = inner_half_size(sdf.border_widths);
    let inner_offset = border_center_offset(sdf.border_widths);
    let inner_radii = inner_corner_radii(sdf.border_widths);
    let inner_dist = sd_rounded_box(local - inner_offset, max(inner_hs, vec2(0.0)), inner_radii);

    // Inner fill alpha.
    let inner_aa = aa_width(inner_dist);
    let inner_alpha = 1.0 - smoothstep(-inner_aa, inner_aa, inner_dist);

    // Standard PBR input from the base StandardMaterial.
    var pbr_input = pbr_input_from_standard_material(in, is_front);
    let fill = pbr_input.material.base_color;
    let border = sdf.border_color;
    let has_fill = fill.a > 0.001;

    // ── Border alpha ───────────────────────────────────────────────
    // Classic ring formula: works for thick borders and filled elements.
    let classic_border_alpha = outer_alpha * (1.0 - inner_alpha);

    // Stroke-centerline formula: treats the border as a stroke centered
    // between the outer and inner SDFs. Guarantees alpha=1 at the stroke
    // center, giving TAA a stable opaque pixel to lock onto. Used for
    // thin border-only elements where the classic formula has no solid core.
    let stroke_center = 0.5 * (outer_dist + inner_dist);
    let stroke_half_width = max(0.5 * (inner_dist - outer_dist), 0.0);
    let stroke_aa = max(fwidth(stroke_center), 0.0001);
    let stroke_shape = 1.0 - smoothstep(
        stroke_half_width - stroke_aa,
        stroke_half_width + stroke_aa,
        abs(stroke_center),
    );
    // Sub-pixel coverage fade: prevent persistent hairlines when zoomed out.
    let border_screen = (2.0 * stroke_half_width) / stroke_aa;
    let coverage = smoothstep(0.0, 1.0, border_screen);
    let thin_stroke_alpha = stroke_shape * coverage;

    // Blend between classic and stroke formulas based on whether the
    // border is thick enough for the classic formula to have a solid core.
    var border_alpha = classic_border_alpha;
    if !has_fill && has_border {
        let thin_border_mix = 1.0 - smoothstep(0.75, 1.5, stroke_half_width / stroke_aa);
        border_alpha = mix(classic_border_alpha, thin_stroke_alpha, thin_border_mix);
    }

    // ── Compositing ────────────────────────────────────────────────
    var final_color: vec4<f32>;
    if has_border {
        if has_fill {
            // Filled element with border: mix fill and border via outer_alpha.
            let border_mix = clamp(border_alpha / max(outer_alpha, 0.001), 0.0, 1.0);
            final_color = vec4<f32>(
                mix(fill.rgb, border.rgb, border_mix),
                outer_alpha * mix(fill.a, border.a, border_mix),
            );
        } else {
            // Border-only: composite border color directly with border_alpha.
            // Do not gate through outer_alpha — the stroke path can produce
            // border_alpha > outer_alpha at the shape boundary.
            final_color = vec4<f32>(
                border.rgb,
                border.a * border_alpha,
            );
        }
    } else {
        // Fill only, no border.
        final_color = vec4<f32>(
            fill.rgb,
            fill.a * outer_alpha,
        );
    }

    pbr_input.material.base_color = final_color;

    pbr_input.material.base_color = alpha_discard(
        pbr_input.material,
        pbr_input.material.base_color,
    );

    // Lighting: respect the unlit flag.
    var out: FragmentOutput;
    if (pbr_input.material.flags & STANDARD_MATERIAL_FLAGS_UNLIT_BIT) == 0u {
        out.color = apply_pbr_lighting(pbr_input);
    } else {
        out.color = pbr_input.material.base_color;
    }
    out.color = main_pass_post_lighting_processing(pbr_input, out.color);

    // OIT support for transparent fragments.
    // Offset position.z so coplanar layers get distinct depths in the
    // OIT linked list. Pipeline depth_bias does NOT affect in.position.z,
    // so we apply the offset here before oit_draw stores the fragment.
#ifdef OIT_ENABLED
    let alpha_mode = pbr_input.material.flags & STANDARD_MATERIAL_FLAGS_ALPHA_MODE_RESERVED_BITS;
    if alpha_mode != STANDARD_MATERIAL_FLAGS_ALPHA_MODE_OPAQUE {
        var oit_pos = in.position;
        oit_pos.z += sdf.oit_depth_offset;
        oit_draw(oit_pos, out.color);
        discard;
    }
#endif

    return out;
}
#endif
