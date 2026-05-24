# Slug text effects

## Executive summary

**Decision.** Slug becomes hana's sole text renderer; the SDF / MSDF
/ MTSDF path is retired. Of the text effects a signed distance field
makes cheap, none *requires* one — each is reachable from slug's
analytic coverage, and several are cheaper or more correct that way.

**Governing constraint.** Slug is fragment-bound (per-pixel curve
evaluation). The winning implementation of every effect pushes work
*off* the per-pixel path — onto one-time CPU geometry, a separate
pipeline pass, or the font itself — rather than multiplying per-pixel
evaluations.

| # | Effect | Chosen approach | Distance field? |
|---|--------|-----------------|-----------------|
| 1 | Outline / stroke | Geometric contour offset (own a stroke-to-fill routine) | No |
| 2 | Glow / halo | Isolated bloom camera, or offscreen separable blur | No |
| 3 | Drop shadow | Real cast shadow (depth-offset geometry + scene light), or faked blur reusing the glow primitive; hard-draw floor | No |
| 4 | Bold + weight animation | Variable fonts (axis interpolation) | No |
| — | Italic | Slant axis (variable) or skew fallback | No |
| — | Underline / strikethrough | Metric-positioned quads — not a glyph effect | No |
| 5 | Inner shadow / emboss / bevel | Real extruded + beveled geometry (faked blur-gradient fallback) | No |
| 6 | Rounded / eroded edges | One-time contour fillet (or #1 offset close); noise on coverage for distress | No |
| 7 | Knockout / legibility halo | Compose #1 stroke + #2 halo in contrasting color; auto-contrast policy; opt-in per WorldText | No |

**The one engineering prerequisite: variable-font support.** It is
the only item that needs real pipeline work rather than a render
technique, and it underpins Bold, weight animation, and (via slant)
Italic. Current state: not supported, but the stack is ready.

- Outline side: call `Face::set_variation(wght / slnt / …)` before
  `outline_glyph` in `slug_text_spike/geometry.rs`. The
  `variable-fonts` feature is on by default in ttf-parser 0.25, it
  normalizes axis coordinates internally, and `outline_glyph`
  respects the set axes — so it is a single call before extraction,
  not a manual-normalization task.
- Shaping side: thread the same axis coordinates through parley's
  variation style settings, so advances match the heavier outlines.
- Cache: extend the slug glyph cache key (currently font id + glyph
  id) to include axis coordinates, so instances at different weights
  do not collide.
- Italic: a slant axis when the font has one, else a skew-transform
  fallback. Underline and strikethrough are metric-positioned quads,
  unrelated to outlines or coverage.

So variable-font support is wiring existing capability, not adding a
parser or a new library.

## Purpose

This document evaluates the text effects that a signed distance
field (SDF / MSDF / MTSDF) renderer makes cheap, and asks, for each
one, whether the slug renderer — which produces analytic *coverage*
rather than a distance field — can produce the same effect, by what
technique, and at what cost.

The motivating decision: retire the SDF / MSDF / MTSDF path in favor
of slug. Slug prepares far faster (no atlas generation) and is
analytically precise at any zoom; it pays a slightly higher
per-pixel render cost at scale. The only reason to keep a distance
field around is an effect that structurally requires one. This
document works through the candidates one at a time to find out
whether any do.

The seven effects under review:

1. Outline / stroke
2. Glow / halo
3. Soft drop shadow
4. Weight animation (dilate / erode)
5. Inner shadow / emboss / bevel
6. Rounded or eroded edges
7. Knockout / legibility halo

Each gets its own section. Shared reasoning that applies to all of
them lives in the performance model below.

## Performance model

This section states the constraints every effect section relies on,
so they are written once here rather than repeated in each.

### Slug is fragment-bound

Slug stores each glyph as Bézier contours partitioned into
horizontal and vertical bands. The fragment shader selects the band
covering a pixel and tests only the curves crossing that band.
Render cost is therefore dominated by *curves-per-band × pixels
shaded* — it is per-pixel work. The vertex and submission stages are
not the bottleneck. (The recent band-count reduction, 96 → 48, and
the per-curve dedup between horizontal and vertical bands both
attack curves-per-band, which is the right axis.)

Consequence: the axis to protect is the per-pixel evaluation count.
Any technique that multiplies how many curve evaluations each pixel
performs is multiplying the one cost slug is already weakest on
relative to SDF.

### Coverage is not distance

SDF stores, at every texel, the signed distance to the nearest glyph
edge. Slug computes *coverage*: the fraction of a pixel that falls
inside the glyph. These carry different information. A pixel two
pixels outside the edge reads coverage 0 and gives no hint of how
far the edge is; the same pixel in an SDF carries an exact distance.
Most SDF effects are a threshold or smoothstep on that distance.
Slug has to either reconstruct an approximate distance (extra
per-pixel samples) or sidestep the need for one (geometry done ahead
of time).

### Governing principle

Because slug is fragment-bound, the winning implementation of any
effect pushes work *off* the per-pixel path — onto one-time CPU
geometry, which is slug's cheap axis — and avoids raising the
per-pixel evaluation count. Effects that reduce to a one-time
geometry step are nearly free at render time. Effects that are
inherently a per-pixel neighborhood read (spatial spreads) cannot
move off the fragment path and are the hard cases.

### Render-run instancing is not a lever here

Slug currently instances per glyph. Moving to per-run instancing
changes the instance / vertex / submission stage, which is not the
bottleneck, so it does not reduce the per-pixel cost of the base
render or of any effect. Its only fragment-relevant benefit is
overdraw: a run-level quad could shade pixels in glyph-overlap
regions (kerning, overhangs, diacritics) once instead of twice. But
a run quad also shades the whitespace between glyphs and adds a
per-pixel "which glyph is this" lookup, so for ordinary text it
trades overlap double-shading for whitespace shading — likely a wash
or a loss. It is worth revisiting only if profiling shows heavy
glyph overlap; it is not a route to cheaper effects.

## 1. Outline / stroke

### 1.1 The three variants

"Outline" names three geometrically distinct results:

- **Outer stroke** adds pixels *outside* the existing glyph
  boundary. The fill is untouched; a band is added around it, so the
  glyph's visual footprint grows. This is the default stroked-text
  look.
- **Inner stroke** adds pixels *inside* the boundary. The footprint
  is unchanged; the outline eats into the fill along the edge.
- **Center stroke** straddles the edge, half outside and half
  inside.

An outline of width N is, in every case, the region between two
boundary contours: an outer boundary and an inner boundary. The
three variants differ only in where those two boundaries sit
relative to the original contour.

### 1.2 Why the obvious approaches fail

**Scaling a larger copy behind the fill does not produce a stroke.**
A stroke is a uniform perpendicular offset: every point on the
contour moves outward along its own normal by the same N pixels.
Scaling moves every point radially away from a center by an amount
proportional to its distance from that center. The two coincide only
for a circle. Concrete failures: scaling the bar of an "I" or an
em-dash by 1.1× pushes its long edges out far and its short ends out
little, giving a fat outline on the flanks and a hairline on the
ends; scaling a glyph with a counter ("o", "e", "a") enlarges the
counter too, so the inner edge of the outline lands on the wrong
side once the fill is composited on top; tight inter-stroke gaps
distort unevenly. Scaling is the wrong operation.

**Thresholding a single coverage sample does not produce an outer
stroke.** This is the coverage-is-not-distance constraint from the
performance model. Outside the glyph, coverage is flat zero, so
there is no value to threshold against to find "within N pixels of
the edge." An inner stroke is more tractable, because it lives
entirely where coverage is already nonzero, but even it is not a
single clean threshold the way an SDF inner stroke is.

### 1.3 Viable techniques

Three techniques can produce an outline from slug. Each is
characterized by its per-pixel cost and which of the three variants
it supports.

**Geometric contour offset.** Offset the glyph's Bézier contours
outward and / or inward by the stroke width, then render the offset
contour as a second slug pass. The offset contour is an ordinary
slug contour with its own bands, so outline-ring pixels evaluate it
once and fill pixels evaluate the original once; only the thin
boundary overlap pays twice. Per-pixel cost is therefore roughly 1×
plus the outline-ring area — well under 2× — and it adds no extra
evaluations to any single pixel. It supports all three variants from
one routine: an outline is the band between two boundary contours,
and the variant is selected by the sign and magnitude of each
boundary's offset (outer: +N and 0; inner: 0 and −N; center: +N/2
and −N/2).

**Multi-tap in-shader.** In a single pass over an enlarged bounding
box, sample coverage at the center plus a ring of offset positions
and combine them to reconstruct an approximate distance to the edge,
then threshold it. This reproduces the SDF approach from coverage,
and once an approximate distance exists it can threshold any of the
three variants. But it evaluates coverage N times per pixel —
multiplying the per-pixel cost by the tap count — and smooth, uniform
width needs many taps (16–32), so width quality is tap-limited.

**Offset-copy union.** Render the glyph several times at small
positional offsets arranged in a ring, all in the outline color,
then the fill on top. Unioning translated copies approximates a
Minkowski sum with a disk, which is a true uniform stroke. It needs
no geometry math, but it costs N× the per-pixel evaluation over the
dilated region (eight or more copies), and because union only
*grows* a region it produces the outer variant only — inner and
center require erosion, which a union of copies cannot express.

### 1.4 Verdict

Geometric contour offset wins, and the fragment-bound constraint is
why. It is the only technique that does not raise the per-pixel
evaluation count: multi-tap multiplies it by the tap count and
offset-copy union multiplies it by the copy count, both attacking
the axis slug is already weakest on, while the offset approach keeps
every pixel at a single evaluation and moves the real work to a
one-time CPU geometry step — slug's cheap axis. It is also the only
technique that produces all three variants from one parameterized
routine at full quality.

### 1.5 Cost to build

The price of the geometric approach is owning a stroke-to-fill
contour-offset routine. Outward offset requires corner-join handling
(miter, round, or bevel decisions at convex corners). Inward offset
requires detecting and trimming self-intersections, since offsetting
inward collapses thin stems and crosses the contour against itself.
This is a solved problem — it is exactly what a vector renderer does
to convert a stroked path into a fillable region — but it is real
geometry code to write and own. The decision for this effect reduces
to whether outlined text is wanted in hana enough to justify writing
that routine; if it is, the render-time cost is modest and the
fragment-bound budget is preserved.

## 2. Glow / halo

A glow is a soft luminous falloff radiating outward from the glyph,
decaying with distance over a radius far larger than a stroke. It is
a spatial spread: each output pixel's intensity depends on its
neighborhood, not on a single coverage sample. The multi-tap
reconstruction that served the outline is a dead end here — a glow
radius is large, so the tap ring would be large and need a high tap
count to stay smooth, multiplying per-pixel evaluations, which is the
worst move when fragment-bound. The answer in every case below is to
move the spread *off* slug's band-evaluation path.

Two facts make this easy. First, a glow is low-frequency by
definition, so slug's analytic precision is moot — the glow can be
produced from a blurred or low-resolution source with no visible
loss. Second, the spread can ride pipeline stages that never touch
slug's per-pixel curve evaluation.

Whole-scene camera bloom is assumed available and is not documented
here — it is a standard post-process applied to the frame. What this
section documents is **glow on text isolated from the rest of the
scene**, by two paths, plus the cheapest path for text that is not
static.

### 2.1 Isolated glow via a dedicated bloom camera and render layer

Place the glow text on its own render layer, rendered by a second
HDR camera carrying a bloom component, composited in order over the
main view. Bloom blurs only the bright pixels on that layer, so the
glow is isolated from every other element in the scene — the
luminance-gating that makes whole-scene bloom indiscriminate is
contained to one sparse layer.

Cost is one extra camera pass over a near-empty layer plus its bloom
pass; on slug's side the text is rendered once, so the per-pixel
curve budget is untouched. The glow takes the text's own emissive
color, and its radius and intensity are the bloom settings on that
camera — shared by everything on the layer, so finer per-element
control means more layers. Diegetically this is the physical model:
emissive glyphs bleeding light, consistent with a lit panel
responding to PBR, with no unlit hack. Reach for this path when the
glow is the text's own color and the physical light-bleed look is
what is wanted.

### 2.2 Standalone spawned blur effect — the cheapest path for non-static text

For text that relayouts or animates, a cached distance field is off
the table: it would have to be rebuilt every frame (JFA per frame),
which loses to a blur outright. The cheapest route is an offscreen
render plus a separable Gaussian blur, spawned as an effect bound to
the text entity and living and dying with it.

Render the text to a *downsampled* offscreen target — a glow is
low-frequency, so a half- or quarter-resolution target is enough and
cuts the blur cost proportionally — then run a two-pass separable
Gaussian (horizontal, then vertical) and composite the result behind
the crisp fill. Confine the work to the text's bounding region
rather than the full frame. Because the text is not static, the
target is re-rendered and re-blurred on the frames it changes, but
the cost is bounded by the small downsampled target and the two
separable passes, and none of it lands on slug's band-evaluation
path.

This path gives full art direction: a glow color independent of the
fill, per-element radius and intensity, and it works on text that is
not emissive at all. Reach for it when the glow must be off-color or
individually tuned, or when the text is not bright enough to drive
bloom.

### 2.3 Verdict

Glow does not argue for keeping the SDF / MSDF / MTSDF renderer.
Both documented paths move the spread off slug's per-pixel curve
evaluation — one onto an isolated bloom camera, the other onto
texture-space convolution — and both keep the fragment budget
intact. The coarse cached distance field, weighed earlier, is not
used here: it pays off only for static text with an animated radius
reused across several effects, and the case documented here is
explicitly non-static text and isolated glow. Slug's lost precision
is irrelevant to a soft effect, so nothing is given up.

## 3. Soft drop shadow

A soft drop shadow is a blurred copy of the glyph, offset by some
(dx, dy), tinted dark, and composited *behind* the fill.
Mechanically it is glow plus an offset and a dark tint, so much of
section 2 carries straight over — with one exception. The dedicated
bloom camera does not apply: bloom is additive and luminance-gated,
it makes bright things glow, and a shadow is dark and subtractive,
so there is no bloom analog. There are instead two routes worth
supporting for users who need proper shadows, plus a cheap floor.

### 3.1 Real cast shadow

On a diegetic PBR panel, raise the text geometry slightly off the
panel surface in Z and the scene lighting casts an actual soft
shadow through the shadow-map pass already running for the panel. No
faking: the shadow's direction and softness come from the panel's
own light. This is the physical model, the same logic that makes
emissive plus bloom the right glow for a lit panel — the shadow is a
real consequence of the text sitting above the surface.

Cost is the depth-offset geometry plus the shadow pass that PBR
already pays for; there is no slug-side work and no per-pixel curve
cost. The tradeoffs are that it is light-driven rather than
art-directed — the offset, softness, and tint are dictated by the
scene light, not set by hand — and that thin raised text geometry
can be awkward for shadow maps, needing bias tuning to avoid
peter-panning and resolution artifacts. Use this when the shadow
should obey the panel's light and read as the text physically
floating above the surface.

### 3.2 Faked art-directed soft shadow

This reuses the standalone blur primitive from section 2.2
unchanged: render the text to a downsampled offscreen target, run a
two-pass separable Gaussian blur, then composite the blurred copy at
an offset in the shadow color, behind the crisp fill. A soft shadow
is that primitive plus three parameters — offset, tint, and behind
placement — so once 2.2 exists, the shadow is a parameterization of
it rather than a new system.

This route is fully art-directed: arbitrary offset and direction,
softness set by the blur radius, any tint and opacity, all
independent of the scene lighting, and it works on text that is
neither emissive nor lit. Its cost is exactly the glow blur path's —
a small downsampled target and two separable passes, paid per frame
for non-static text, none of it on slug's band-evaluation path. Use
this when the shadow must be tuned independently of the lighting, or
when the text is not lit or bright.

### 3.3 Hard-shadow floor

With the blur radius at zero the faked path degenerates to a single
offset, dark-tinted slug draw composited behind the fill — the
cheapest shadow there is. It suits contact shadows and crisp UI drop
shadows where softness is not wanted, and it costs one extra slug
draw with no blur passes at all.

### 3.4 Verdict

Two supported routes cover proper shadows: the real cast shadow for
the diegetic "text floating above the panel" look, and the faked
blur shadow for full art direction, with the hard draw as the cheap
floor. None of them needs a distance field or the SDF / MSDF / MTSDF
renderer, and the faked path shares its entire implementation with
glow. Guidance for users: choose the physical cast shadow when the
shadow should track the panel's own light, and the faked shadow when
it must be tuned independently of lighting or the text is not lit.

## 4. Weight animation, Bold, Italic, underline, strikethrough

These are font-level concerns, not coverage effects, and they group
here because they all change the glyph at the source rather than in
a post pass. A weight change produces a *sharp* result — a heavier
glyph still has crisp edges — so the low-frequency blur escape used
for glow and shadow does not apply, and slug's precision matters.
The enabling feature for the whole group is variable-font support,
detailed in the executive summary.

### 4.1 Weight and Bold

A variable font packs master outlines along a weight axis, and any
weight between Regular and Bold is the designer-interpolated blend
of the masters' control points. Bold is a high weight value;
intermediate weights are designed, not faked. Animating weight is
therefore a per-frame control-point lerp along the axis — cheap CPU
work with no joins or self-intersections to resolve — rendered at 1×
in slug.

This is worth stating plainly: SDF's celebrated "free" weight
animation is its distance-threshold dilation, which is uniform faux
bold — typographically wrong, because a real bold thickens stems
more than round parts and protects counters. Slug with a variable
font produces the designer's actual bold and every weight between,
so on this effect slug is strictly better than the renderer being
retired, not merely equal.

Without a variable font, the fallback is the geometric contour
offset from section 1 (positive offset to dilate), which reproduces
faux bold at 1× fragment cost, paying per-frame CPU geometry when
animated continuously or caching discrete steps. A crossfade between
a Regular and a Bold face is a cruder fallback that reads as a fade
rather than a weight change.

### 4.2 Italic

Real italic comes from a slant axis on a variable font, or a
separate italic face — both carry designed letterforms (a true
italic often redraws letters, such as a single-story "a"). Faux
italic is a horizontal skew of the glyph transform: cheap, works on
any font, but with no redrawn letterforms. Prefer the designed form
when the font provides it; the skew is the universal fallback.

### 4.3 Underline and strikethrough

These are not glyph effects at all. They are lines drawn at
metrics-defined positions — underline at the font's underline
position below the baseline, strikethrough through the x-height —
one filled quad per run spanning its width. Animating an underline
(a reveal, say) is animating that quad's width or opacity. No
coverage, no outline, no offset routine is involved.

### 4.4 The prerequisite

Everything above except the faux fallbacks depends on variable-font
wiring: `set_variation` before outline extraction, the matching axis
coordinates threaded through parley's shaping, and the axis
coordinates added to the slug glyph cache key. The change points are
listed in the executive summary.

## 5. Inner shadow / emboss / bevel

Three related faux-3D depth looks: an **inner shadow** is a dark
gradient cast inside the glyph from one edge, as if the fill is
recessed below the surface; **emboss / engrave** makes the glyph
read as raised or pressed-in via a light edge on one side of the
contour and a dark edge on the other; a **bevel** is an angled edge
ramp around the boundary, shaded as 3D, needing a surface normal
that varies across the bevel width.

This is the effect where a distance field fits most naturally — a
bevel is the distance field lit as a heightfield, with distance
giving depth into the bevel and the distance gradient giving the
edge normal to light. It is SDF's strongest case. Even so, on a
physical panel the correct answer is not a faked heightfield; it is
real geometry.

### 5.1 Real extruded and beveled geometry

Extrude the glyph into 3D geometry with a chamfered edge — raised
for emboss, recessed for engrave — and let the scene light it
through the PBR pass already running for the panel. The normals are
real, the directional response is real, the inner shadow of an
engraved glyph is real self-shadowing rather than a painted
gradient. This rides the planned real-geometry support for bevy
diegetic: emboss and bevel become a consequence of that geometry
existing, not a separate effect system. Cost is real text geometry
(extrusion and bevel tessellation, more vertices) lit by the
existing PBR pass; nothing lands on slug's per-pixel curve path.

It is the same physical pattern established for glow (emissive plus
bloom) and drop shadow (depth-offset cast shadow): the effect is a
property of the glyph being a physical object on the panel,
consistent with the panel-content-is-physical constraint. This is
the recommended route, and it is the one hana intends to build.

### 5.2 Faked blurred-coverage gradient

Where real geometry is not available — flat text, or before the
geometry feature lands — render coverage to an offscreen target,
blur it, and use the screen-space gradient of the blurred coverage
as a pseudo-normal: the blur width becomes the bevel width, and
lighting that pseudo-normal directionally gives emboss and bevel.
The inner shadow is the blurred complement clipped inside the fill.
It is approximate and flat (no real depth or parallax), but holds up
for a stylized look, and it stays in texture space, off slug's band
evaluation. A narrow-band coarse distance field (JFA, only a few
pixels wide near the edge) is a cleaner-normal variant of the same
idea at more passes — worth it only if the blurred gradient is too
soft and real geometry is off the table.

### 5.3 Verdict

Even SDF's strongest case does not justify keeping the SDF / MSDF /
MTSDF renderer. The correct emboss or bevel on a physical panel is
real lit geometry, which hana plans to add regardless, so SDF's
natural fit for a faked heightfield is not the route that would
actually be taken. Where a cheap flat bevel is wanted without
geometry, the blurred-coverage gradient reaches it with no distance
field. A distance field would only marginally sharpen the faked
version; it is neither required nor the chosen path.

## 6. Rounded or eroded edges

Two unrelated looks travel under this heading: rounding the sharp
corners of glyphs, and an eroded or worn distressing of the edge.
They want different machinery, and neither needs a distance field or
a lighting pass.

### 6.1 Rounded corners

Softening sharp corners (corner fillets, ink-trap rounding) is a
contour-geometry operation: do it once, then render normally.

The direct route replaces each sharp corner of the Bézier contour
with a small arc of the rounding radius — a one-time CPU step — after
which slug renders the rounded contour at 1×. It also falls out of
the stroke-to-fill offset routine from section 1: offsetting a
contour outward then back inward (a morphological close) rounds
corners with a radius equal to the offset, the same
Minkowski-with-a-disk rounding that SDF gets by thresholding an
offset. So if the offset routine exists, rounded corners are nearly
free; a direct fillet is simply cleaner. Either way the result is
analytic and precise — better than field-resolution-limited SDF
rounding — and the cost sits on slug's cheap one-time-geometry axis,
not the per-pixel path.

### 6.2 Worn or eroded distress

Irregular nibbling of the edge for a degraded, grunge, or aged look
is a per-pixel stochastic perturbation, and it works directly on
coverage with no distance field. For subtle distress, perturb the
coverage cutoff with a noise sample so the edge erodes irregularly —
one noise sample per pixel applied right after coverage, off the
band-evaluation path. Because slug's coverage transition is about a
pixel wide, this nibbles within roughly a pixel, which suits fine
distress. For a chunkier, wider erosion, widen the transition first
with the offscreen blur primitive, then noise-threshold the wide
ramp — still no distance field, just the blur plus a noise sample.

### 6.3 Verdict

Neither look needs a distance field or the SDF / MSDF / MTSDF
renderer. Rounded corners are one-time contour geometry — a direct
fillet or the section 1 offset routine's close operation — and worn
distress is a cheap noise modulation of coverage, widened by the
blur primitive when chunkiness is wanted. There is no lighting or
physical route here; if real geometry from section 5 is in play, a
worn look could instead ride a roughness or normal texture on that
mesh, but flat slug text uses the noise route.

## 7. Knockout / legibility halo

This is not a new primitive but a composition of the outline
(section 1) and glow (section 2) primitives deployed for readability
rather than decoration — and it is a feature hana will likely want
to *enable* on a WorldText at minimum. Its job is to keep text
legible over busy, unpredictable, or low-contrast backgrounds: world
or HUD text drawn over arbitrary scene content, where the fill color
alone cannot be trusted to stay readable.

### 7.1 The pieces

Three building blocks, each already specified elsewhere, used in a
contrasting color:

- **Contrasting outer stroke** — a thin outline in a color that
  contrasts the fill (a dark ring on light text, or the reverse),
  guaranteeing edge contrast against whatever is behind. This is the
  section 1 geometric contour offset at a small radius.
- **Soft contrasting halo** — a blurred spread behind the text,
  centered rather than offset, in a contrasting color, lifting the
  text off the background. This is the section 2 offscreen blur
  primitive. The emissive-plus-bloom glow path does not apply, since
  a legibility halo is usually dark.
- **Text plate / knockout backing** — at the strong end, a filled,
  blurred backing in the run's silhouette, darkened and composited
  behind the text, so the text sits on its own semi-opaque slab
  regardless of the background.

### 7.2 Variations

The pieces combine along a strength continuum:

- Thin contrasting stroke alone — lightest touch, guaranteed edge
  contrast, cheapest.
- Stroke plus soft halo — the robust default, readable over most
  backgrounds.
- Halo widened into a full plate or knockout — strongest, for the
  busiest or least predictable backgrounds, at the cost of obscuring
  more of what is behind.

All three reuse the stroke-to-fill offset routine and the offscreen
blur primitive verbatim; none needs a distance field or the SDF /
MSDF / MTSDF renderer, and none adds per-pixel curve evaluation
beyond those primitives.

### 7.3 Auto-contrast policy

What separates a legibility halo from a decorative outline is
intent-driven contrast, which can be automated rather than tuned per
scene. Sample the background luminance behind the run — the
framebuffer region the run covers — and choose the stroke and halo
to be dark-on-light or light-on-dark so the text stays readable
without a manual color choice. For text or backgrounds that move,
the sample is taken per frame; for static placements it can be
resolved once. This is a policy layer over the same primitives, not
a rendering technique, and it is what lets the feature "just work"
when enabled.

### 7.4 Enablement

The natural model is an opt-in feature on the text entity — enabled
on a WorldText at minimum — that switches the halo on with its
variation (stroke, halo, or plate) and its policy (a fixed
contrasting color, or auto-contrast). It lives and dies with the
text entity, the same pattern as the spawned glow and shadow
effects, and it composes the already-built outline and blur
primitives rather than introducing anything new.

### 7.5 Verdict

The legibility halo is the highest-value effect in this list for
readability, and it costs nothing beyond the outline offset routine
and the blur primitive already specified for effects 1 and 2. No
distance field, no SDF renderer; the only addition over plain
outline-plus-glow is the auto-contrast policy that picks the halo
color from the background.
