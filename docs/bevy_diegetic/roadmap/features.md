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
| Font asset loading       | `FontLoader` (`AssetLoader` for .ttf/.otf), `FontRegistered`/`FontLoadFailed` events, `FontSource`, `font_id_by_name()` lookup |
| Glyph loading policy     | `GlyphLoadingPolicy` (WhenReady default, Progressive opt-in), `MsdfAtlas::preload()` for atlas warming |
| Non-ASCII glyphs         | Validated via `atlas_pages` example rendering Latin Extended Unicode blocks across multiple pages |

## Phase 4 — Font ligatures & foundations

| # | Feature                   | Notes                                                              |
|---|---------------------------|--------------------------------------------------------------------|
| 1 | Font ligatures            | ~~Support OpenType GSUB ligatures.~~ Done — `FontFeatureFlags` (liga, calt, dlig, kern), parley shaping. |
| 2 | Panel rendering           | ~~Real geometry replaces gizmo wireframes.~~ Done — panels spawn `WorldText` children via `reconcile_panel_text_children`, unified readiness model with `PendingGlyphs` / `WorldTextReady`. |
| 3 | Physical font sizing      | ~~Default `WorldText` scale matches real-world point sizes.~~ Done — `METERS_PER_POINT`, `TextScale` resource, `TextScaleOverride` per-entity. Superseded by #4 (Unit System). See [PHYSICAL_FONT_SIZING.md](PHYSICAL_FONT_SIZING.md). |
| 4 | World/Text units          | ~~`Unit` enum, `UnitConfig`, per-panel overrides, pre-scale to points.~~ Done — See [UNIT_SYSTEM.md](UNIT_SYSTEM.md). |
| 4b | Unit system DX           | Newtypes (`Pt`, `Mm`, `In`), `PaperSize` enum, `world_width`/`world_height` scaling. Design panels like documents, scale to world size. See [UNIT_SYSTEM_DX.md](UNIT_SYSTEM_DX.md). |
| 4c | Example unit migration   | Apply the unit system + DX layer to all examples with proper scaling. |
| 5 | Type-safe font resolution | `ResolvedFont` couples font_id with font_data. Placeholder rendering with honest identity prevents atlas poisoning. Reactive `FontRegistered` observer swaps to real font when loaded. See [TYPE_SAFE_REACTIVE_FONTS.md](TYPE_SAFE_REACTIVE_FONTS.md). |
| 6 | Text config cascade       | Default `TextConfig` on `DiegeticPanel` or any `El` container. All `b.text()` calls inside inherit it — child containers can override. Cascades like CSS inherited properties. Unlocks ergonomic styling for Phases 5–9. |
| 7 | CJK font example          | Load Noto Sans JP, render Japanese text as `WorldText`. Validates multi-font + non-ASCII + multi-page atlas end-to-end. |

## Phase 5 — Text decoration

| # | Feature             | Notes                                                              |
|---|---------------------|--------------------------------------------------------------------|
| 5 | Underline           | Horizontal line below text baseline. Establishes the decoration quad pattern: metrics from parley (underline offset/thickness), generated quad per text run. |
| 6 | Strikethrough       | Horizontal line through text center. Same approach as underline with different vertical offset. |
| 7 | Drop shadow         | Per-text shadow as a second render pass with offset and color. Currently done manually in the shadows example. Should be a `TextConfig` option. |

## Phase 6 — Font maturity & variants

| # | Feature                          | Notes                                                              |
|---|----------------------------------|--------------------------------------------------------------------|
| 8  | Font weight/variant enumeration | Query by family name, list available weights (Extra Light through Black) from `os2` metadata. |
| 9  | Bold text                       | Select bold weight variant from registered font family. Requires #8. Driven by `TextConfig` or rich text markup. |
| 10 | Italic text                     | Select italic/oblique slant variant from registered font family. Requires #8. Driven by `TextConfig` or rich text markup. |
| 11 | Multi-font / system fonts       | Enumerate system fonts, query weight/slant variants per family. |
| 12 | Async font preview rasterization | Background task rasterizes glyphs for each font name in its own font. Font picker menu appears instantly. |

## Phase 7 — Panel polish

| # | Feature              | Notes                                                              |
|---|----------------------|--------------------------------------------------------------------|
| 13 | Corner radius       | Rounded rect shader. 4-corner independent radii. **Done.**        |
| 13b | SDF fragment clipping | Walk parent hierarchy, pass clip bounds + corner radius to fragment shader. Prerequisite for scroll containers, widgets, and any content overflow. (Inspired by univis_ui `UClip`.) |
| 13c | Fit-content roots   | Panel auto-sizes to content with optional min/max constraints. Eliminates manual width/height for simple panels. (Inspired by univis_ui `fit_content_world_3d`.) |
| 13d | Chamfered corners   | `CornerStyle` enum: `Rounded` (existing) or `Cut` (diagonal chamfer). Small SDF shader delta. (Inspired by univis_ui `Shape::Cut`.) |
| 14 | Image elements      | Image content in panel elements.                                   |
| 15 | Typography overlay  | Feature-gated. `TypographyOverlay` component — font metric lines, per-glyph bounding boxes. In progress. |
| 16 | Panel layout overlay | Feature-gated. `LayoutOverlay` component — sizing modes, padding, alignment, borders. Color-coded by sizing mode. |

## Phase 8 — Interaction & layout

| # | Feature              | Notes                                                              |
|---|----------------------|--------------------------------------------------------------------|
| 17a | Flex wrap           | `FlexWrap` / `WrapReverse` on containers. Layout primitive needed before scroll containers — wrapping is the simpler overflow case. (Inspired by univis_ui `UFlexWrap`.) |
| 17b | Extended alignment  | `space-between`, `space-around`, `space-evenly` for child distribution along main axis. Baseline alignment for cross-axis text alignment. (Inspired by univis_ui CSS-like alignment.) |
| 17 | Scroll containers   | Scroll offset, content size tracking. Needed for overflow. Unblocked by SDF clipping (13b). |
| 18 | Scroll in 3D        | Scroll containers driven by 3D raycasts, not mouse events.        |
| 19 | Text alignment       | Per-text-element Left/Center/Right alignment.                     |
| 20 | Floating elements    | Attach points, z-index, pointer capture.                          |
| 21 | Pointer/hover state  | Track hovered element. In 3D this means raycasting.               |
| 21b | Interaction states  | Auto-tint elements based on hover/press state. `InteractionColors` component for idle/hovered/pressed color transitions. (Inspired by univis_ui `UInteractionColors`.) |
| 22 | Element IDs          | String IDs for debug, pointer targeting, scroll.                  |
| 23 | Aspect ratio         | `aspect_ratio()` on elements.                                     |
| 23b | Grid layout         | CSS Grid with tracks, auto-flow, and explicit cell placement. Major layout feature. (Inspired by univis_ui Grid display.) |
| 23c | Stack layout        | Overlapping children with z-index ordering. (Inspired by univis_ui Stack display.) |
| 23d | Masonry layout      | Pinterest-style columns where items fill vertical gaps. (Inspired by univis_ui Masonry display.) |
| 23e | Radial layout       | Children arranged in a circle or arc. (Inspired by univis_ui Radial display.) |

## Phase 9 — Rich text & effects

| # | Feature                | Notes                                                              |
|---|------------------------|--------------------------------------------------------------------|
| 24 | Rich text / inline markup | `Text3d::rich("{red:WARNING} -- {green:all clear}")`            |
| 25 | Dynamic text segments  | Live-updating text from ECS entities.                             |
| 26 | Per-glyph effects      | Wave, shake, typewriter, fade — indexed arrays + per-glyph entities. |
| 27 | Text outline           | Render text as outline strokes instead of filled glyphs. Configurable stroke width and color via `TextConfig`. Requires MSDF edge extraction or secondary SDF pass. |
| 28 | Text truncation / ellipsis | Detect overflow, replace tail with "...".                      |
| 29 | Auto-fit text sizing   | Shrink font to fit container. `clamp(min, max)`, best-fit binary search. |

## Phase 10 — 3D text geometry

| # | Feature                  | Notes                                                              |
|---|--------------------------|----------------------------------------------------------------------|
| 30 | Glyph outline extraction | Implement `ttf_parser::OutlineBuilder` to extract bezier curves from glyph outlines. Flatten to polylines with adaptive subdivision. |
| 31 | 2D glyph tessellation    | Tessellate flattened glyph outlines into triangle meshes using `lyon_tessellation`. Handle holes (counter-wound inner contours) via EvenOdd fill. Using lyon directly rather than `fontmesh` — `fontmesh` is just a thin wrapper around lyon + `OutlineBuilder` anyway, and the delta (adapter + extrusion fn) is straightforward to own. `lyon_tessellation` is the de facto standard (4M+ downloads, actively maintained). |
| 32 | 3D text extrusion        | Extrude tessellated 2D glyph faces into 3D meshes — front face, back face, side walls with edge-perpendicular normals. Configurable depth. Spawn as Bevy `Mesh3d` with standard material for lighting/shadows. |
| 33 | Text string layout       | Position extruded glyphs using advance widths and kerning from `ttf-parser`. Horizontal layout with proper spacing. Reuse parley shaping where possible. |
| 34 | Glyph mesh caching       | Cache tessellated/extruded meshes per (glyph, font, depth) to avoid re-tessellation. |

## Phase 11 — Polish

| # | Feature                | Notes                                                              |
|---|------------------------|--------------------------------------------------------------------|
| 35 | Custom element data   | Arbitrary data on render commands.                                 |
| 35b | Root isolation       | Prevent z-fighting and interaction leaking between separate world-space UI trees. `RenderLayers` handles screen-space; world-space needs explicit depth management. (Inspired by univis_ui root capsules.) |
| 36 | Per-side border colors | Currently uniform color only.                                     |
| 37 | Baseline offset        | MSDF quads have extra space below baseline — investigate when visually noticeable. |
| 38 | Debug gizmos → overlay | Replace `ShowTextGizmos` with panel-rendered debug overlay.       |
| 39 | Performance observability | Stabilize `DiegeticPerfStats`, decouple from internal system names, integrate with Bevy `DiagnosticsStore`. |

## Phase 12 — Rich text and syntax highlighting

| # | Feature                | Notes                                                              |
|---|------------------------|--------------------------------------------------------------------|
| 40 | Rich text spans       | Per-span styling (color, weight, slant) within a single `b.text()` call. Enables mixed formatting in one text element without multiple elements. |
| 41 | Syntax highlighting   | `syntect` integration for code coloring. Parse code strings into colored spans, render via rich text. Per-glyph vertex colors already supported by the MSDF renderer. |

## Phase 13 — Widget library

Depends on: interaction states (#21, #21b), SDF clipping (#13b), scroll containers (#17).
Inspired by univis_ui's built-in widget set.

| # | Feature                | Notes                                                              |
|---|------------------------|--------------------------------------------------------------------|
| 42 | Button               | Clickable element with interaction state callbacks. Builds on #21. |
| 43 | Toggle / Checkbox    | Boolean state control with visual indicator.                       |
| 44 | Radio group          | Mutual-exclusion selection across a set of options.                |
| 45 | Seekbar / Slider     | Continuous value input via drag. Needs pointer capture (#20).      |
| 46 | Drag value           | Numeric input adjusted by dragging (Blender-style). Variant of #45. |
| 47 | Select / Dropdown    | Pop-up option list. Needs clipping (#13b) and floating (#20).      |
| 48 | Text input field     | Editable text with cursor, selection, and basic editing. Major feature. |
| 49 | Progress bar         | Visual progress indicator. Simple — percentage fill of a container. |
| 50 | Scroll view (widget) | Packaged scroll container with scrollbar chrome. Builds on #17.    |
| 51 | Icon button          | Button with an image/icon instead of text. Builds on #14 and #42.  |
| 52 | Badge / Pill         | Small status indicator label.                                      |
| 53 | Menu                 | Hierarchical menu with submenus. Needs floating (#20) and clipping (#13b). |
| 54 | Divider (widget)     | Formalize the existing layout-primitive divider as a reusable widget. |

## Phase 14 — Distribution

| # | Feature                | Notes                                                              |
|---|------------------------|--------------------------------------------------------------------|
| 55 | Publish to crates.io | Stabilize public API, add crate-level docs, publish.               |
