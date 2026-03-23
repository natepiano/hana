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
| 7 | Underline           | Horizontal line below text baseline. Needs metrics from parley (underline offset/thickness) and a generated quad per text run. |
| 8 | Strikethrough       | Horizontal line through text center. Same approach as underline with different vertical offset. |
| 9 | Drop shadow         | Per-text shadow as a second render pass with offset and color. Currently done manually in the shadows example. Should be a `TextConfig` option. |
| 10 | Text config cascade | Default `TextConfig` on `DiegeticPanel` or any `El` container. All `b.text()` calls inside inherit it — child containers can override. Cascades like CSS inherited properties. |

## Phase 6 — Panel rendering

| # | Feature              | Notes                                                              |
|---|----------------------|--------------------------------------------------------------------|
| 11 | Panel rendering     | Real geometry replaces gizmo wireframes. Mesh quads for backgrounds/borders. |
| 12 | Corner radius       | Rounded rect shader. 4-corner independent radii.                   |
| 13 | Image elements      | Image content in panel elements.                                   |
| 14 | Typography overlay  | Feature-gated. `TypographyOverlay` component — font metric lines, per-glyph bounding boxes. In progress. |
| 15 | Panel layout overlay | Feature-gated. `LayoutOverlay` component — sizing modes, padding, alignment, borders. Color-coded by sizing mode. |

## Phase 7 — Interaction & layout

| # | Feature              | Notes                                                              |
|---|----------------------|--------------------------------------------------------------------|
| 16 | Scroll containers   | Scroll offset, content size tracking. Needed for overflow.         |
| 17 | Scroll in 3D        | Scroll containers driven by 3D raycasts, not mouse events.        |
| 18 | Text alignment       | Per-text-element Left/Center/Right alignment.                     |
| 19 | Floating elements    | Attach points, z-index, pointer capture.                          |
| 20 | Pointer/hover state  | Track hovered element. In 3D this means raycasting.               |
| 21 | Element IDs          | String IDs for debug, pointer targeting, scroll.                  |
| 22 | Aspect ratio         | `aspect_ratio()` on elements.                                     |

## Phase 8 — Rich text & effects

| # | Feature                | Notes                                                              |
|---|------------------------|--------------------------------------------------------------------|
| 23 | Rich text / inline markup | `Text3d::rich("{red:WARNING} -- {green:all clear}")`            |
| 24 | Dynamic text segments  | Live-updating text from ECS entities.                             |
| 25 | Per-glyph effects      | Wave, shake, typewriter, fade — indexed arrays + per-glyph entities. |
| 26 | Text truncation / ellipsis | Detect overflow, replace tail with "...".                      |
| 27 | Auto-fit text sizing   | Shrink font to fit container. `clamp(min, max)`, best-fit binary search. |

## Phase 9 — Polish

| # | Feature                | Notes                                                              |
|---|------------------------|--------------------------------------------------------------------|
| 28 | Custom element data   | Arbitrary data on render commands.                                 |
| 29 | Per-side border colors | Currently uniform color only.                                     |
| 30 | Baseline offset        | MSDF quads have extra space below baseline — investigate when visually noticeable. |
| 31 | Debug gizmos → overlay | Replace `ShowTextGizmos` with panel-rendered debug overlay.       |
| 32 | Performance observability | Stabilize `DiegeticPerfStats`, decouple from internal system names, integrate with Bevy `DiagnosticsStore`. |
