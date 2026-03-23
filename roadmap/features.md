# bevy_diegetic Feature Roadmap

## Phase 1 — Replace bevy_rich_text3d (done)

| Step                     | Notes                                                       |
|--------------------------|-------------------------------------------------------------|
| Typestate types          | `TextProps<C>`, `TextConfig`, `TextStyle`, `TextMeasure`    |
| Font registry            | `FontRegistry` with embedded `JetBrains Mono`, parley 0.7   |
| Parley measurer          | `MeasureTextFn` backed by parley (real shaping/kerning)     |

## Phase 2 — MSDF atlas pipeline (done)

| Step                     | Notes                                                       |
|--------------------------|-------------------------------------------------------------|
| MSDF atlas pipeline      | `fdsm` + `etagere`, async on-demand rasterization           |
| msdfgen comparison       | fdsm 2-4x faster than C++ msdfgen, 15% parity              |

## Phase 3 — Text rendering (done)

| Step                     | Notes                                                       |
|--------------------------|-------------------------------------------------------------|
| Text rendering           | MSDF quads render, positioning correct, side-by-side integrated |
| Glyph render modes       | `GlyphRenderMode` (Invisible, Text, PunchOut, SolidQuad) and `GlyphShadowMode` (None, SolidQuad, Text, PunchOut) with shadow proxy pipeline, prepass fragment shader, per-batch material grouping |
| Per-element text color   | Text color driven by `TextConfig`, per-glyph vertex colors  |
| Multi-page atlas         | `AtlasPage` struct, automatic overflow, per-page materials, `page_count()`/`glyph_count()` diagnostics |
| Atlas config API         | `RasterQuality` enum, `AtlasConfig` with `glyphs_per_page`, `DiegeticUiPlugin::with_atlas()` builder |
| Async on-demand glyphs   | `AsyncComputeTaskPool` rasterization, `GlyphsReady` flag, `SharedMsdfMaterials` invalidation, non-destructive rebuild |
| Non-ASCII glyphs         | Validated via `atlas_pages` example rendering Latin Extended Unicode blocks across multiple pages |

## Phase 4 — Font system

| # | Feature                          | Notes                                                              |
|---|----------------------------------|--------------------------------------------------------------------|
| 1 | `register_font` API             | `FontRegistry::register_font(data: &[u8]) -> FontId` — synchronous registration of additional fonts. Requires threading font data lookup through atlas `get_or_insert` and async rasterization (currently hardcoded to `EMBEDDED_FONT`). Foundation for everything below. |
| 2 | CJK font example                | Load Noto Sans JP via `register_font`, render Japanese text as `WorldText`. Validates multi-font + non-ASCII + multi-page atlas. |
| 3 | Font ligatures                  | Support OpenType GSUB ligatures (`::`, `->`, `=>`). Requires multi-character cluster quads spanning the full ligature width. Currently falls back to cmap glyph IDs. |
| 4 | Multi-font / system fonts       | Enumerate system fonts, query weight/slant variants per family. Build on `register_font`. |
| 5 | Font weight/variant enumeration | Query by family name, list available weights (Extra Light through Black) from `os2` metadata. Google Docs-style font picker. |
| 6 | Async font preview rasterization | Background task rasterizes glyphs for each font name in its own font. Font picker menu appears instantly. |

## Phase 5 — Text decoration & cascade

| # | Feature             | Notes                                                              |
|---|---------------------|--------------------------------------------------------------------|
| 7 | Bold text           | Select bold weight variant from registered font family. Requires Phase 4 `register_font` + weight enumeration. Driven by `TextConfig` or rich text markup. |
| 8 | Italic text         | Select italic/oblique slant variant from registered font family. Requires Phase 4 font variant enumeration. Driven by `TextConfig` or rich text markup. |
| 9 | Underline           | Horizontal line below text baseline. Needs metrics from parley (underline offset/thickness) and a generated quad per text run. |
| 10 | Strikethrough       | Horizontal line through text center. Same approach as underline with different vertical offset. |
| 11 | Drop shadow         | Per-text shadow as a second render pass with offset and color. Currently done manually in the shadows example. Should be a `TextConfig` option. |
| 12 | Text config cascade | Default `TextConfig` on `DiegeticPanel` or any `El` container. All `b.text()` calls inside inherit it — child containers can override. Cascades like CSS inherited properties. |

## Phase 6 — Panel rendering

| # | Feature              | Notes                                                              |
|---|----------------------|--------------------------------------------------------------------|
| 13 | Panel rendering     | Real geometry replaces gizmo wireframes. Mesh quads for backgrounds/borders. |
| 14 | Corner radius       | Rounded rect shader. 4-corner independent radii.                   |
| 15 | Image elements      | Image content in panel elements.                                   |
| 16 | Typography overlay  | Feature-gated. `TypographyOverlay` component — font metric lines, per-glyph bounding boxes. In progress. |
| 17 | Panel layout overlay | Feature-gated. `LayoutOverlay` component — sizing modes, padding, alignment, borders. Color-coded by sizing mode. |

## Phase 7 — Interaction & layout

| # | Feature              | Notes                                                              |
|---|----------------------|--------------------------------------------------------------------|
| 18 | Scroll containers   | Scroll offset, content size tracking. Needed for overflow.         |
| 19 | Scroll in 3D        | Scroll containers driven by 3D raycasts, not mouse events.        |
| 20 | Text alignment       | Per-text-element Left/Center/Right alignment.                     |
| 21 | Floating elements    | Attach points, z-index, pointer capture.                          |
| 22 | Pointer/hover state  | Track hovered element. In 3D this means raycasting.               |
| 23 | Element IDs          | String IDs for debug, pointer targeting, scroll.                  |
| 24 | Aspect ratio         | `aspect_ratio()` on elements.                                     |

## Phase 8 — Rich text & effects

| # | Feature                | Notes                                                              |
|---|------------------------|--------------------------------------------------------------------|
| 25 | Rich text / inline markup | `Text3d::rich("{red:WARNING} -- {green:all clear}")`            |
| 26 | Dynamic text segments  | Live-updating text from ECS entities.                             |
| 27 | Per-glyph effects      | Wave, shake, typewriter, fade — indexed arrays + per-glyph entities. |
| 28 | Text outline           | Render text as outline strokes instead of filled glyphs. Configurable stroke width and color via `TextConfig`. Requires MSDF edge extraction or secondary SDF pass. |
| 29 | Text truncation / ellipsis | Detect overflow, replace tail with "...".                      |
| 30 | Auto-fit text sizing   | Shrink font to fit container. `clamp(min, max)`, best-fit binary search. |

## Phase 9 — 3D text geometry

| # | Feature                  | Notes                                                              |
|---|--------------------------|----------------------------------------------------------------------|
| 31 | Glyph outline extraction | Implement `ttf_parser::OutlineBuilder` to extract bezier curves from glyph outlines. Flatten to polylines with adaptive subdivision. |
| 32 | 2D glyph tessellation    | Tessellate flattened glyph outlines into triangle meshes using `lyon_tessellation`. Handle holes (counter-wound inner contours) via EvenOdd fill. Using lyon directly rather than `fontmesh` — `fontmesh` is just a thin wrapper around lyon + `OutlineBuilder` anyway, and the delta (adapter + extrusion fn) is straightforward to own. `lyon_tessellation` is the de facto standard (4M+ downloads, actively maintained). |
| 33 | 3D text extrusion        | Extrude tessellated 2D glyph faces into 3D meshes — front face, back face, side walls with edge-perpendicular normals. Configurable depth. Spawn as Bevy `Mesh3d` with standard material for lighting/shadows. |
| 34 | Text string layout       | Position extruded glyphs using advance widths and kerning from `ttf-parser`. Horizontal layout with proper spacing. Reuse parley shaping where possible. |
| 35 | Glyph mesh caching       | Cache tessellated/extruded meshes per (glyph, font, depth) to avoid re-tessellation. |

## Phase 10 — Polish

| # | Feature                | Notes                                                              |
|---|------------------------|--------------------------------------------------------------------|
| 36 | Custom element data   | Arbitrary data on render commands.                                 |
| 37 | Per-side border colors | Currently uniform color only.                                     |
| 38 | Baseline offset        | MSDF quads have extra space below baseline — investigate when visually noticeable. |
| 39 | Debug gizmos → overlay | Replace `ShowTextGizmos` with panel-rendered debug overlay.       |
| 40 | Performance observability | Stabilize `DiegeticPerfStats`, decouple from internal system names, integrate with Bevy `DiagnosticsStore`. |
