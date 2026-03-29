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
    /// Half-size of the element in world units (width/2, height/2).
    half_size:     vec2<f32>,
    /// Per-corner radii in world units: [TL, TR, BR, BL].
    corner_radii:  vec4<f32>,
    /// Border widths in world units: [top, right, bottom, left].
    border_widths: vec4<f32>,
    /// Border color in linear RGBA.
    border_color:  vec4<f32>,
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
fn inner_half_size() -> vec2<f32> {
    return vec2<f32>(
        sdf.half_size.x - 0.5 * (sdf.border_widths.y + sdf.border_widths.w),
        sdf.half_size.y - 0.5 * (sdf.border_widths.x + sdf.border_widths.z),
    );
}

/// Computes the offset from center due to asymmetric border widths.
fn border_center_offset() -> vec2<f32> {
    return vec2<f32>(
        0.5 * (sdf.border_widths.w - sdf.border_widths.y),
        0.5 * (sdf.border_widths.x - sdf.border_widths.z),
    );
}

/// Shrinks corner radii by the minimum border width on adjacent sides.
fn inner_corner_radii() -> vec4<f32> {
    return max(
        vec4(0.0),
        vec4<f32>(
            sdf.corner_radii.x - min(sdf.border_widths.x, sdf.border_widths.w), // TL
            sdf.corner_radii.y - min(sdf.border_widths.x, sdf.border_widths.y), // TR
            sdf.corner_radii.z - min(sdf.border_widths.z, sdf.border_widths.y), // BR
            sdf.corner_radii.w - min(sdf.border_widths.z, sdf.border_widths.w), // BL
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
    let local = (in.uv - 0.5) * 2.0 * sdf.half_size;

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
}
#else

// ── Main pass ───────────────────────────────────────────────────────

@fragment
fn fragment(
    in: VertexOutput,
    @builtin(front_facing) is_front: bool,
) -> FragmentOutput {
    // Map UV (0..1) to local coordinates centered on the quad.
    let local = (in.uv - 0.5) * 2.0 * sdf.half_size;

    // Clip to parent scissor rect.
    if local.x < sdf.clip_rect.x || local.x > sdf.clip_rect.z
        || local.y < sdf.clip_rect.y || local.y > sdf.clip_rect.w {
        discard;
    }

    // Outer shape distance.
    let outer_dist = sd_rounded_box(local, sdf.half_size, sdf.corner_radii);

    // Per-distance AA width from screen-space derivatives of the SDF.
    let outer_aa = aa_width(outer_dist);

    // Outer shape alpha — smooth falloff at the edge.
    let outer_alpha = 1.0 - smoothstep(-outer_aa, outer_aa, outer_dist);

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
    let inner_hs = inner_half_size();
    let inner_offset = border_center_offset();
    let inner_radii = inner_corner_radii();
    let inner_dist = sd_rounded_box(local - inner_offset, max(inner_hs, vec2(0.0)), inner_radii);

    // Inner fill alpha.
    let inner_aa = aa_width(inner_dist);
    let inner_alpha = 1.0 - smoothstep(-inner_aa, inner_aa, inner_dist);

    // Border alpha: between outer and inner edges.
    let border_alpha = outer_alpha * (1.0 - inner_alpha);

    // Standard PBR input from the base StandardMaterial.
    var pbr_input = pbr_input_from_standard_material(in, is_front);

    // Composite: fill color from base_color, border color from uniform.
    var final_color: vec4<f32>;
    if has_border {
        // Mix fill and border.
        let fill = pbr_input.material.base_color;
        let border = sdf.border_color;

        // Fill inside, border on the edge, transparent outside.
        final_color = vec4<f32>(
            mix(fill.rgb, border.rgb, border_alpha / max(outer_alpha, 0.001)),
            outer_alpha * mix(fill.a, border.a, border_alpha / max(outer_alpha, 0.001)),
        );
    } else {
        // Fill only.
        final_color = vec4<f32>(
            pbr_input.material.base_color.rgb,
            pbr_input.material.base_color.a * outer_alpha,
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
