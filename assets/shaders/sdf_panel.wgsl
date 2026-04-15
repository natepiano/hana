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

#import bevy_diegetic::sdf_stroke::{
    centered_stroke_alpha,
    inflate_subpixel_half_size,
    stable_border_alpha,
    stable_line_alpha,
}

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
    /// Shape selector. `0` = rounded rect, `1` = triangle, `2` = circle,
    /// `3` = diamond, `4` = line segment.
    shape_kind:    u32,
    /// Extra shape parameters for custom SDF shapes.
    shape_params:  vec4<f32>,
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

/// Distance to a line segment from `a` to `b`.
fn sd_segment(p: vec2<f32>, a: vec2<f32>, b: vec2<f32>) -> f32 {
    let pa = p - a;
    let ba = b - a;
    let h = clamp(dot(pa, ba) / dot(ba, ba), 0.0, 1.0);
    return length(pa - ba * h);
}

/// Signed distance to a right-pointing isosceles triangle whose tip is
/// at `(+half_size.x, 0)` and whose base spans `(-half_size.x, ±half_size.y)`.
fn sd_triangle(p: vec2<f32>, half_size: vec2<f32>) -> f32 {
    let a = vec2<f32>(half_size.x + sdf.shape_params.x, 0.0);
    let b = vec2<f32>(-half_size.x, half_size.y);
    let c = vec2<f32>(-half_size.x, -half_size.y);

    let d = min(sd_segment(p, a, b), min(sd_segment(p, b, c), sd_segment(p, c, a)));

    let s1 = (b.x - a.x) * (p.y - a.y) - (b.y - a.y) * (p.x - a.x);
    let s2 = (c.x - b.x) * (p.y - b.y) - (c.y - b.y) * (p.x - b.x);
    let s3 = (a.x - c.x) * (p.y - c.y) - (a.y - c.y) * (p.x - c.x);
    let has_neg = s1 < 0.0 || s2 < 0.0 || s3 < 0.0;
    let has_pos = s1 > 0.0 || s2 > 0.0 || s3 > 0.0;
    let inside = !(has_neg && has_pos);

    return select(d, -d, inside);
}

/// Signed distance to a centered circle.
fn sd_circle(p: vec2<f32>, half_size: vec2<f32>) -> f32 {
    return length(p) - min(half_size.x, half_size.y);
}

/// Signed distance to a horizontal line segment with thickness
/// `2 * half_size.y`, centered on the X axis and spanning
/// `[-half_size.x, +half_size.x]`.
fn sd_line_segment(p: vec2<f32>, half_size: vec2<f32>) -> f32 {
    let a = vec2<f32>(-half_size.x, 0.0);
    let b = vec2<f32>(half_size.x, 0.0);
    return sd_segment(p, a, b) - half_size.y;
}

fn line_center_distance(p: vec2<f32>, half_size: vec2<f32>) -> f32 {
    let a = vec2<f32>(-half_size.x, 0.0);
    let b = vec2<f32>(half_size.x, 0.0);
    return sd_segment(p, a, b);
}

/// Signed distance to a right-pointing diamond.
fn sd_diamond(p: vec2<f32>, half_size: vec2<f32>) -> f32 {
    let a = vec2<f32>(half_size.x, 0.0);
    let b = vec2<f32>(0.0, half_size.y);
    let c = vec2<f32>(-half_size.x, 0.0);
    let d = vec2<f32>(0.0, -half_size.y);

    let dist = min(
        min(sd_segment(p, a, b), sd_segment(p, b, c)),
        min(sd_segment(p, c, d), sd_segment(p, d, a)),
    );

    let s1 = (b.x - a.x) * (p.y - a.y) - (b.y - a.y) * (p.x - a.x);
    let s2 = (c.x - b.x) * (p.y - b.y) - (c.y - b.y) * (p.x - b.x);
    let s3 = (d.x - c.x) * (p.y - c.y) - (d.y - c.y) * (p.x - c.x);
    let s4 = (a.x - d.x) * (p.y - d.y) - (a.y - d.y) * (p.x - d.x);
    let has_neg = s1 < 0.0 || s2 < 0.0 || s3 < 0.0 || s4 < 0.0;
    let has_pos = s1 > 0.0 || s2 > 0.0 || s3 > 0.0 || s4 > 0.0;
    let inside = !(has_neg && has_pos);

    return select(dist, -dist, inside);
}

/// Shape dispatch for callouts/panels sharing the same SDF backend.
fn sd_shape(p: vec2<f32>, half_size: vec2<f32>, radii: vec4<f32>) -> f32 {
    if sdf.shape_kind == 1u {
        return sd_triangle(p, half_size);
    }
    if sdf.shape_kind == 2u {
        return sd_circle(p, half_size);
    }
    if sdf.shape_kind == 3u {
        return sd_diamond(p, half_size);
    }
    if sdf.shape_kind == 4u {
        return sd_line_segment(p, half_size);
    }
    return sd_rounded_box(p, half_size, radii);
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

fn shape_aa_width(dist: f32) -> f32 {
    if sdf.shape_kind == 1u {
        return aa_width(dist) * max(0.1, sdf.shape_params.y);
    }
    return aa_width(dist);
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

    let dist = sd_shape(local, sdf.half_size, sdf.corner_radii);

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
        let min_shadow_widths = vec4<f32>(pixel_size.y, pixel_size.x, pixel_size.y, pixel_size.x);
        let shadow_border_widths = select(
            vec4<f32>(0.0),
            max(sdf.border_widths, min_shadow_widths),
            sdf.border_widths > vec4<f32>(0.0),
        );
        let inner_hs = inner_half_size(shadow_border_widths);
        let inner_offset = border_center_offset(shadow_border_widths);
        let inner_radii = inner_corner_radii(shadow_border_widths);
        let inner_dist = sd_shape(local - inner_offset, max(inner_hs, vec2(0.0)), inner_radii);
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
    let inflated = inflate_subpixel_half_size(sdf.half_size, pixel_size);
    let effective_half_size = inflated.xy;
    let coverage_scale = inflated.z;

    let is_line_shape = sdf.shape_kind == 4u;
    let line_center_dist = line_center_distance(local, effective_half_size);
    let line_outer_dist = line_center_dist - effective_half_size.y;
    let line_inner_dist = line_center_dist + effective_half_size.y;
    let line_outer_aa = max(fwidth(line_outer_dist) * 0.75, 0.0001);
    let line_outer_alpha = 1.0 - smoothstep(0.0, 2.0 * line_outer_aa, line_outer_dist);
    let line_alpha = select(
        0.0,
        stable_border_alpha(line_outer_alpha, line_outer_dist, line_inner_dist),
        is_line_shape,
    );

    if is_line_shape && line_alpha < 0.001 {
        discard;
    }

    // Outer shape distance (using potentially inflated half-size).
    let outer_dist = sd_shape(local, effective_half_size, sdf.corner_radii);

    // Base AA width from screen-space derivatives of the SDF.
    let base_aa = shape_aa_width(outer_dist);

    // Outer shape alpha — exterior-only falloff with doubled ramp width.
    // Interior pixels (dist ≤ 0) are fully opaque so adjacent quads
    // sharing an edge don't double-blend and produce visible seams.
    // The 2× ramp compensates for the one-sided shift, preserving the
    // same visual AA band width as a centered smoothstep.
    let outer_alpha = (1.0 - smoothstep(0.0, 2.0 * base_aa, outer_dist)) * coverage_scale;

    // Discard fully outside fragments.
    if !is_line_shape && outer_alpha < 0.001 {
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
    let inner_dist = sd_shape(local - inner_offset, max(inner_hs, vec2(0.0)), inner_radii);

    // Inner fill alpha.
    let inner_aa = shape_aa_width(inner_dist);
    let inner_alpha = 1.0 - smoothstep(-inner_aa, inner_aa, inner_dist);

    // Standard PBR input from the base StandardMaterial.
    var pbr_input = pbr_input_from_standard_material(in, is_front);
    let fill = pbr_input.material.base_color;
    let border = sdf.border_color;
    let has_fill = fill.a > 0.001;

    // ── Border alpha ───────────────────────────────────────────────
    // Classic ring formula: works for thick borders and filled elements.
    let classic_border_alpha = outer_alpha * (1.0 - inner_alpha);
    let thin_stroke_alpha = centered_stroke_alpha(outer_dist, inner_dist);

    // Blend between classic and stroke formulas based on whether the
    // border is thick enough for the classic formula to have a solid core.
    var border_alpha = classic_border_alpha;
    if !has_fill && has_border {
        let stroke_center = 0.5 * (outer_dist + inner_dist);
        let stroke_half_width = max(0.5 * (inner_dist - outer_dist), 0.0);
        let stroke_aa = max(fwidth(stroke_center), 0.0001);
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
    } else if is_line_shape {
        final_color = vec4<f32>(
            fill.rgb,
            fill.a * line_alpha,
        );
    } else {
        // Fill only, no border.
        final_color = vec4<f32>(
            fill.rgb,
            fill.a * outer_alpha,
        );
    }

    if final_color.a < 0.001 {
        discard;
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
