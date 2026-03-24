# bevy_diegetic Text System -- Vision

bevy_diegetic already has a complete layout engine -- a Clay-inspired algorithm reimplemented in pure Rust, fully thread-safe, no global state, no unsafe. It computes where every rectangle and text element goes on an in-world panel.

What it does not yet have is a way to *draw* those elements. This document describes the text rendering system that fills that gap.

## Mission

Build the definitive 3D text system for Bevy. Not a port of a 2D toolkit, not a wrapper around an existing rasterizer -- a text system designed from the ground up for entities that live inside the game world.

The bar is TextMeshPro for Unity, but native to Bevy's ECS and Rust's type system. Sharp at any distance. Composable effects without special APIs. Per-glyph access for anyone who wants it.

## Philosophy

### Text is not special

In Processing, text is "just another thing you draw." You set a font, set a size, call `text()`, and it appears on the canvas alongside rectangles and ellipses. Effects like wave motion or per-character color don't require a text effect system -- you measure each character's position, apply a transform, and draw it.

bevy_diegetic follows the same principle. A glyph is a positioned quad with a material. It lives in the ECS like any other entity. If you want to make a character wobble, you write a system that modifies its `Transform`. If you want per-character color, you set its material. The text system provides measurement and placement; Bevy provides everything else.

### Measurement before rendering

The layout engine already separates *what goes where* from *how it looks*. Text rendering follows the same split. First: measure glyphs and compute their positions in layout space. Second: render them using whatever technique fits (MSDF quads, mesh geometry, bitmap fallback). This separation means layout never depends on the GPU, and rendering never re-derives positions the layout engine already computed.

### Effects from composition, not configuration

There is no `TextEffect::Hologram` enum variant. There is no `GlitchIntensity` field on a text component. Instead, each glyph's position, size, and character are exposed via `ComputedGlyphLayout`. A hologram effect is a system that applies a hologram material to the text entity. A typewriter effect is a system that reveals glyphs over time by writing opacities to `GlyphTransforms`. A glitch effect is a system that jitters glyph offsets on a timer. These are ordinary Bevy systems -- not special-purpose text APIs.

This means any effect anyone can imagine is possible without modifying bevy_diegetic. The crate provides the atoms; users compose the molecules.

## Core Tenets

**Sharp at any scale.** MSDF (multi-channel signed distance field) rendering is the baseline. Text on a panel across the room and text on a panel you're holding in your hand must both be crisp. Bitmap rasterization blurs. SDF alone can't represent sharp corners. MSDF solves both. This is the same technique behind TextMeshPro and Godot 4's text rendering.

**Dead-simple API.** Spawning text on a panel should take one component. Changing its content should be one field assignment. The common case -- put some text on a thing in the world -- must be trivial. Complexity is available, never required.

**Per-glyph ECS integration.** After layout, every glyph's position, size, and character data are exposed via a `ComputedGlyphLayout` component. Per-character effects are achieved through a `GlyphTransforms` component that holds per-glyph offsets, colors, and opacities -- no entity-per-glyph needed for the common case. For advanced use cases, per-glyph entities can optionally be spawned as lightweight children (metadata only, no mesh). [TECHNICAL NOTE: per-glyph entities are opt-in to avoid entity count explosion; the default path uses indexed data arrays for performance.]

**Performance-first.** Text that isn't changing doesn't cost anything. Bevy's change detection means layout recomputation is skipped entirely on frames where the panel hasn't been touched. MSDF atlases are generated once per font and reused. Glyph meshes are instanced. The system should handle hundreds of panels with thousands of glyphs without becoming a bottleneck.

**Diegetic-native.** Every design decision assumes text lives in 3D space, attached to surfaces, floating in the world, viewed from arbitrary angles and distances. Self-lit and emissive materials by default so text reads without external lighting. Dark backplates for contrast. Distance-based LOD so far-away panels don't waste fill rate on unreadable text. Optional billboarding for text that should always face the camera.

**Accessible.** Font size scaling as a global multiplier. High-contrast mode that forces maximum foreground/background separation. Configurable fonts so users can substitute dyslexia-friendly typefaces. These aren't afterthoughts bolted on later -- they're parameters from day one.

## What It Enables

**Diegetic UI panels.** Health bars on enemies, ammo counters on weapons, status displays on control panels, price tags on shop items -- any interface element that exists as a physical object in the game world rather than a HUD overlay. The layout engine handles arrangement; the text system handles rendering.

**In-world terminals and consoles.** Scrolling log output on a computer screen embedded in a level. Command prompts. Boot sequences. The retained-mode layout engine means updating a single line doesn't recompute the entire panel.

**Holographic displays.** Floating text with emissive materials, scan-line effects, and transparency. Because effects are just systems that modify glyph entities, a hologram is a material swap and a subtle vertex offset -- not a hardcoded rendering path.

**Creative coding in 3D.** Processing-style experimentation where text is a first-class visual element. Kinetic typography. Data visualization with labeled axes. Generative art with letterforms. The per-glyph ECS representation means every character is individually addressable and animatable.

**Signage and environmental storytelling.** Street signs, graffiti, warning labels, alien alphabets. Text baked into the world with proper lighting response (or deliberate self-illumination). Because rendering is material-based, the same glyph can look painted, neon, carved, or projected depending on the material assigned.

## What Sets It Apart

**MSDF rendering as the default path.** Most Bevy text solutions rasterize to bitmaps. bevy_diegetic uses multi-channel signed distance fields for resolution-independent rendering. Sharp edges at every distance, tiny atlas footprint, and GPU-driven effects (outlines, shadows, glow) that come nearly free via the distance field.

**Per-glyph access is a first-class concept.** Other text systems render entire text blocks as single meshes with no way to address individual characters. bevy_diegetic exposes per-glyph positions and metadata via `ComputedGlyphLayout`, and per-glyph transform overrides via `GlyphTransforms`. Effects are ordinary Bevy systems that read positions and write offsets -- no special framework needed. The default path uses indexed arrays for performance; per-glyph entities are available as an opt-in for advanced use cases (see CONSIDERATIONS.md section 4).

**Layout and rendering are fully decoupled.** The layout engine runs without any rendering backend. The rendering system consumes layout results without knowing how they were computed. This means you can swap rendering techniques (MSDF, mesh geometry, bitmap fallback) per-panel or per-element without touching layout code.

**Retained-mode layout in an ECS world.** The `LayoutTree` is built once, stored on a component, and recomputed only when Bevy's change detection fires. In a scene with 200 static panels, the layout system does zero work per frame. This is the opposite of immediate-mode approaches that rebuild every frame regardless of changes.

**Pure Rust, no FFI.** No C dependencies, no global state, no thread-safety footguns. Multiple layout engines can run concurrently on different threads. The entire text pipeline -- from font loading through atlas generation through glyph placement -- is safe Rust that plays nicely with Bevy's parallel scheduler.

## Non-Goals

**This is not a text editor.** No cursor management, no selection highlighting, no undo/redo, no input method editor support. If you need editable text fields, build them on top of the glyph entities this system provides, or use a dedicated crate.

**This is not a 2D UI toolkit.** bevy_diegetic is for text and panels that exist in 3D world space. It does not replace `bevy_ui` for screen-space HUD elements. If your text lives on a 2D overlay anchored to screen coordinates, use Bevy's built-in UI.

**This is not a document renderer.** No pagination, no footnotes, no multi-column reflow, no inline images mixed with text. The layout engine handles rectangular containers with text content. Rich document layout is a different problem.

**This is not a font editor or font management system.** Users provide fonts; the system consumes them. Font discovery, fallback chains across language scripts, and system font enumeration are out of scope. The system accepts font data and produces atlases and glyphs.

**This does not aim for pixel-identical cross-platform rendering.** Shaping and rasterization may produce subtly different results across GPU vendors and operating systems. The goal is *visually correct and sharp*, not *bit-identical*.

## Rendering Strategy

The system supports multiple rendering backends behind a common interface, chosen per-panel or per-element:

1. **MSDF quads** (default) -- Each glyph is a textured quad sampling from a multi-channel signed distance field atlas. Sharp at any distance. Ideal for most diegetic UI: status panels, floating labels, HUD-style readouts. GPU-driven outline, shadow, and glow effects via the distance field.

2. **Mesh geometry** (opt-in) -- Glyph outlines tessellated into actual 3D meshes. For cases where text needs true geometric depth: extruded lettering, text that casts real shadows, text carved into surfaces. More expensive but physically correct.

3. **Bitmap rendering** (emoji and color fonts) -- Pre-rasterized glyphs in a texture atlas, using swash's color outline and color bitmap support. Required for emoji, color fonts (COLR/CPAL, CBDT/CBLC), and any glyph that carries inherent color. MSDF is monochrome by nature, so bitmap is the first-class path for color glyphs, not a fallback.

The layout engine doesn't care which backend renders a given element. It produces `RenderCommand` values (each containing a `BoundingBox` and a `RenderCommandKind`) with text metadata; the rendering layer picks the technique.

## The Path from Here

The layout engine is complete and tested. Gizmo wireframes prove the math works. The road forward:

1. **MSDF atlas generation.** Offline and runtime. Accept a font, produce a texture atlas with MSDF data per glyph and the metrics needed for placement.

2. **Glyph placement.** Consume `RenderCommandKind::Text` output from the layout engine, look up glyph metrics from the atlas, emit positioned quads (or entities) in world space.

3. **MSDF shader.** Fragment shader that samples the MSDF atlas and produces sharp anti-aliased edges. Support for outline, shadow, and glow as shader parameters.

4. **Rectangle rendering.** Replace gizmo wireframes with actual mesh-based rectangles with materials, proper depth sorting, and background colors.

5. **Per-glyph entity opt-in.** When requested, decompose a text block's quads into individual entities that systems can query and manipulate.

6. **Effect library.** A collection of example systems (not built-in features) demonstrating typewriter reveal, hologram flicker, wave motion, glitch, CRT curvature. These live in examples, not in the crate -- proving the architecture works by composition rather than configuration.

---

## Related Documents

- **ARCHITECTURE.md** -- Technical architecture: rendering pipeline, atlas generation, font management, module structure.
- **API_DESIGN.md** -- Public API design: components, resources, usage examples, comparison with bevy_rich_text3d.
- **EFFECTS.md** -- Per-glyph effects: two-layer architecture, built-in effects, custom effects, future directions.
- **IMPLEMENTATION.md** -- Phased implementation plan with concrete files, dependencies, and testing strategy.
- **CONSIDERATIONS.md** -- Technical tradeoffs: rendering technique selection, shaping stack, glyph architecture, platform constraints.

---

Game engines have treated 3D text as an afterthought for decades -- a 2D system awkwardly projected into world space. bevy_diegetic treats it as a first-class citizen: measured by a real layout engine, rendered with resolution-independent techniques, and integrated into the ECS so that every glyph is as programmable as every other entity in the world.
