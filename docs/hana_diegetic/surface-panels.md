# Surface Panels — Curved Parametric Surfaces for Diegetic UI

> **Status: DESIGN — not started.** Combined plan for mapping diegetic panels
> (and the headless widgets that live on them) onto curved parametric surfaces
> `S(u,v)`. Coordinates with — but does not replace — the headless widget plan in
> [`widgets.md`](./widgets.md) and the material plan in
> [`tangent-bitangent-normal.md`](./tangent-bitangent-normal.md). This plan
> **generalizes the frame derivation** in that TBN doc: its "Design" section
> synthesizes a *planar* T/B/N; on a curved panel the T/B/N come from `S` per
> vertex. The rest of the TBN doc — normal-map sampling, parallax/relief mapping,
> `uv_transform` handling, material-table classification, batch compatibility — is
> the material **consumer** of this frame and stays as its own plan.

## Goal

Let a `DiegeticPanel` place its content on an arbitrary parametric surface
instead of a flat plane, so a panel — text, fills, borders, images, panel
lines/paths, and headless widgets — wraps onto a cylinder, sphere cap, cone, or
other analytic surface and behaves exactly as it does flat. A headless slider on
a sphere is the capstone target.

The design principle: **flat is the identity case of the surface.** Every code
path that today assumes a plane routes through a single surface abstraction whose
degenerate implementation reproduces current behavior bit-for-bit.

## The one abstraction

Investigation found that the crate has exactly one geometric operation, written
three different ways, each hard-coded to an affine plane:

| Direction | Site | Current form |
| --- | --- | --- |
| Render (forward, per-vertex) | `shaders/sdf_panel.wgsl:226`, `shaders/image_panel.wgsl:87`, `render/analytic_paths/analytic_path_vertex_pull.wgsl:172` — all the identical line `world = record.transform * vec4(local, 0, 1)` | affine plane |
| Anchor (forward, per-point) | `panel/anchor_geometry.rs:463` `PanelPlane::point()` | affine plane |
| Picking (inverse) | `ime/activation.rs:82-98` `panel_local_from_hit` | affine plane inverse, drops `local.z` |

All three are the map **panel-local `(u,v)` ↔ world position + tangent frame**.
Define one trait for it:

```rust
/// Maps normalized panel parameters to world space plus a surface frame.
/// `uv` is in [0,1]^2 over the panel's layout rect (top-left origin, Y-down),
/// so it composes with `Anchor::offset_fraction()` (layout/units/anchor.rs:36).
pub trait PanelSurface {
    fn sample(&self, uv: Vec2) -> SurfaceSample;
    /// Inverse map for hit-testing. `None` if the world point is off-surface.
    fn project(&self, world: Vec3) -> Option<Vec2>;
}

pub struct SurfaceSample {
    pub position: Vec3,
    pub tangent_u: Vec3, // dS/du
    pub tangent_v: Vec3, // dS/dv
    pub normal: Vec3,    // normalize(tangent_u x tangent_v)
}
```

The flat panel is the constant-frame implementation:
`position = origin + u·right·w + v·up·h`, `tangent_u = right`, `tangent_v = up`,
`normal` constant. This reproduces `PanelPlane` (`anchor_geometry.rs:366`)
exactly — that struct is already `{origin, right, up, normal, size}`, i.e. a
frozen `SurfaceSample`.

A `PanelSurface` component (default `Flat`) carries the surface kind and its
parameters (cylinder radius/axis, sphere center/radius, etc.). Adding it to a
panel entity curves that panel; its absence is flat.

## Why the widget layer is curvature-blind

This is the reason a slider on a sphere is tractable rather than a rewrite.

`PanelSurface::project()` converts a world-space pointer hit to panel-local
coordinates at **one** site — the panel boundary. Everything downstream operates
in flat panel-local layout space and never learns the surface exists:

- `ComputedDiegeticPanel::field_at_local_position` (`diegetic_panel.rs:1591`) →
  `PanelFieldRecord::contains` (`panel/field.rs:29`) — an axis-aligned 2D test.
- The planned widget picking, hover, focus, and the slider's panel-local →
  normalized-value mapping (widgets.md concepts 3, 14) — all panel-local.

Symmetrically on output: widgets emit ordinary `El` / `PanelDraw` layout, and the
surface remap happens universally inside the render subsystems, so widget visuals
curve without widget code participating.

**Consequence for the widget plan:** the headless widget behavior and layout
layers need no curvature awareness. Widget picking geometry can stay in
panel-local space; the single per-panel `project()` feeds it curved-correct
coordinates. See "Coordination with widgets.md" below.

## Current Code State

Placement is **shader-side** from a panel transform over panel-local coords —
ideal for a surface remap — but every primitive is a **single un-subdivided
quad**.

- Content is authored flat in panel-local 2D, embedded at `z = 0`, and placed by
  the panel entity's `GlobalTransform`: `world = panel_GlobalTransform * (x, y,
  0, 1)`. The 2D scale/anchor part is centralized in
  `DiegeticPanel::points_to_world()` (`diegetic_panel.rs:564`) and
  `anchor_offsets()` (`:550`), but the `bounds·scale − anchor` / Y-flip formula
  is re-implemented per render subsystem (`render/panel_geometry.rs:865`
  `bounds_to_panel_local_center`, `image_batch.rs:619`,
  `panel_text/reconcile.rs:214`, and the panel path batching module).
- CPU assembles `transform = panel.to_matrix() * local_transform.to_matrix()`
  per record (`fill_batch.rs:444`, `image_batch.rs:162`, panel path batching,
  `panel_text/batching.rs:617/691`); the vertex shader expands each record into
  **4 corners** and computes the world line above.
- The fragment shader evaluates analytic coverage (rounded-box SDF, Bézier
  winding/distance) in flat 2D from an interpolated UV — it never reads world
  position, so coverage stays correct under a remap as long as each quad is small.

Tessellation density today, per primitive:

| Primitive | Density | Facets under curvature? |
| --- | --- | --- |
| Fill / border (SDF) | 1 quad / 4 verts | Yes — full-panel fill chords flat |
| Image | 1 quad / 4 verts | Yes |
| Glyph / text | 1 quad per glyph, whole run one rigid `run.transform` | Glyphs mild; run baseline stays coplanar |
| Panel line / path | 1 merged path + 1 instance quad per merge-group | Yes, worst case — a 100mm spine is one quad, not split |

## Design choice: GPU-analytic surface

Two ways to realize `S`:

- **A. GPU-analytic.** Pass surface kind + params as a uniform; tessellate quads
  CPU-side into sub-grids; the vertex shader evaluates `S(uv)` analytically in
  WGSL per vertex, emitting position + tangent frame. Preserves the crate's cheap
  per-frame re-placement (a moving curved panel just updates its uniform).
  Requires the WGSL `S` to mirror the Rust `S` (shared param struct). Limited to
  analytic surfaces — which covers cylinder, sphere, cone, quadrics: enough for
  the target.
- **B. CPU-baked.** CPU evaluates `S` and writes world positions + normals into
  the buffers; the shader passes them through. Arbitrary meshes/heightfields, one
  Rust implementation — but every panel move re-bakes all vertices, discarding the
  architecture's stable-records / transform-only-update model.

**Decision: A (GPU-analytic) as the primary path.** It fits the existing
stable-record + per-frame-transform model and delivers the analytic-surface set
the goal needs. B stays a documented later extension for arbitrary-mesh surfaces.

Picking does **not** need barycentric UV recovery from the mesh: with an analytic
`project(world) -> uv`, Bevy mesh-picking supplies `HitData.position` (a world
point on the curved interaction mesh) and Rust calls `surface.project()` to get
`uv` → panel-local. The interaction mesh becomes the tessellated curved mesh so
the raycast lands on the real surface; UV recovery stays analytic.

## The three hard problems

1. **Tessellation.** Subdivide each primitive quad into an adaptive sub-grid
   whose density follows the arc-angle the quad subtends on `S`, keeping per-
   sub-quad facet error under a threshold. Fills/borders/images: a grid over the
   quad. Panel line/path merge-groups (the path module and
   `packing.rs:build_packed_polygons`) are the worst case — a group spanning the
   panel needs the group's instance quad subdivided. Glyphs: keep one quad each
   but remap the glyph **origin** through `S` (see 3).

2. **Screen-space AA assumes an affine plane.** Coverage is UV-driven and
   survives the remap; the AA widths do not.
   - SDF `fwidth(local)` AA (`sdf_panel.wgsl:392`) degrades only at high
     curvature-per-quad — tessellation into near-planar sub-quads keeps it valid.
   - The **panel-line AA is the hardest**: `line_aa_margin_local`
     (`analytic_path_vertex_pull.wgsl:95-169`) computes the grazing-angle margin
     from `view.clip_from_world * (run.transform * axis)` — one global affine
     plane. Fix: feed the Jacobian from the **per-vertex surface tangents**
     (`tangent_u`, `tangent_v` from `S`) instead of the constant panel axes. This
     falls out of evaluating `S` per vertex.

3. **Per-vertex normal.** `world_normal` is constant per record in every shader
   (`sdf_panel.wgsl:234`, `image_panel.wgsl:95`, `analytic_path_vertex_pull.wgsl:183`).
   Curved lighting needs `normalize(tangent_u × tangent_v)` per vertex — the same
   frame the AA fix and anchoring need. This is the TBN merge point.

## Phased plan

**Phase 0 — Surface abstraction, flat identity, single boundary.** Define
`PanelSurface`, `SurfaceSample`, `SurfaceKind`, the normalized `[0,1]^2` UV
domain, and the `PanelSurface` component (default `Flat`). Consolidate the
scattered `*_from_bounds` / `local_transform` construction and `PanelPlane` so
they route through the flat surface implementation. No behavior change — pure
refactor establishing the boundary. This is the moderately-invasive-but-mechanical
consolidation the investigation identified.

**Phase 1 — GPU surface eval + tessellation for fills/borders/images.** WGSL
`sample_surface(kind, params, uv) -> SurfaceSample`; surface params in the
record/uniform; CPU adaptive sub-grid tessellation of fill/border/image quads;
vertex shader evaluates `S` per vertex with per-vertex normal; `fwidth` AA
validated on near-planar sub-quads. Prove a curved fill + image on a cylinder and
a sphere cap.

**Phase 2 — Text on surfaces.** Remap each glyph **origin** through `S`
(per-glyph, replacing the per-run rigid `run.transform`, `panel_text/batching.rs:617`);
place each glyph on the local tangent plane; derive glyph aniso-AA axes from the
surface tangents. Prove curved text follows the surface.

**Phase 3 — Panel lines/paths on surfaces.** Per-merge-group tessellation of the
instance quad; **fix the line-AA Jacobian** to use per-vertex surface tangents
(`analytic_path_vertex_pull.wgsl:95-169`). Prove the `units.rs` ruler on a
cylinder.

**Phase 4 — Surface frame feeds TBN.** Emit the per-vertex `T/B/N` from `S` so
lighting normals stop being constant-per-quad. This generalizes the frame source
that `tangent-bitangent-normal.md`'s "Design" section and Implementation-Outline
steps 1–2 assume planar: the synthesized-planar-TBN helper becomes the flat case
of the surface frame. The material feature itself — normal-map sampling,
parallax, `uv_transform` basis, handedness, material-table classification,
validation (TBN doc steps 3–6 and beyond) — is **not** in scope here; it remains
its own plan and consumes whatever frame the panel provides (flat or curved). The
only edit to `tangent-bitangent-normal.md` is to source the frame from `S`.

**Phase 5 — Picking inverse + anchoring.** Swap the flat `PanelInteractionMesh`
`Rectangle` (`panel_geometry.rs:833-861`) for the tessellated curved mesh;
replace `panel_local_from_hit`'s inverse-affine-then-drop-z (`ime/activation.rs:82-98`)
with `surface.project()`. Replace `PanelPlane` in anchor geometry
(`anchor_geometry.rs:366`, `from_panel:383`, `point:463`, `edge:470`) with a
surface-sampled frame keyed by `Anchor::offset_fraction()`; update
`world_anchoring::target_anchor_point` / `plane_rotation` (`world_anchoring.rs:342-399`)
to sample the frame at the anchor's `(u,v)` instead of reading whole-panel
constants. Curved hit-testing and curved-surface tooltips.

**Phase 6 — Widget interop + sphere slider.** Coordinate with widgets.md (below);
demonstrate a headless slider on a sphere. Capstone.

## Coordination with widgets.md

The widget plan stays a separate doc. Curvature reaches widgets through the panel
boundary, so only two touchpoints need coordination, both to avoid re-churning
the same code twice:

1. **Anchoring generalization (widgets.md Phase 0).** That phase renames
   `PanelAnchorGeometryParam -> AnchorGeometryParam` and generalizes anchor
   geometry to resolve from a panel or a materialized widget. Make the generalized
   `AnchorGeometryParam` return an **oriented frame** matching `SurfaceSample`
   (point + tangent frame), not the concrete `PanelPlane`, so this Surface plan's
   Phase 5 does not rewrite it. The frame subsumes screen bounds (2D), the flat
   plane (constant frame), and the curved sampler.

2. **Widget picking geometry (widgets.md concept 3 / Phase 1).** Keep widget
   picking geometry in **panel-local space**. The per-panel `surface.project()`
   already delivers curved-correct panel-local coordinates before any widget
   bounds test, so widget picking needs no curvature logic. Do not place widget
   picking meshes independently in world space.

3. **Demonstration (widgets.md Phase 5).** Add the sphere slider as a
   demonstration target proving headless widget + surface compose.

## Open questions

- UV domain units: normalized `[0,1]^2` over the layout rect vs panel-local
  meters. Normalized composes with `Anchor::offset_fraction()` directly; render
  and picking must apply the panel world-size scale consistently.
- `project()` for surfaces where a world ray hits the surface more than once
  (a full sphere, a closed cylinder): restrict to the front-facing parameter
  patch, or require the interaction mesh to be the single-sided panel patch only.
- Adaptive tessellation density: fixed per-surface heuristic vs curvature-driven
  per-primitive. Start fixed-per-surface; measure faceting; refine.
- Clipping (`DrawOverflow::Clipped`, `clip_rect`) under curvature: the clip test
  is panel-local and survives the remap, but grazing-angle clip AA shares the
  `fwidth` concern.
- Screen-space panels are unaffected (flat by definition); confirm `PanelSurface`
  is rejected or ignored for `CoordinateSpace::Screen`.
